use super::Result;

use difference;

use handlebars::{Handlebars, Helper, RenderContext, RenderError};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File, create_dir_all, OpenOptions};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::io::{self, Read, Write};
use walkdir::WalkDir;

use super::defaults;

/// file to clone template to
// const TMP_PREFIX: &'static str = "porteurbars";
/// subdirectory containing template source
const TEMPLATE_DIR: &'static str = "template";

/// name of file containing key/value pairs representing template defaults
const DEFAULTS: &'static str = "default.env";

/// A template holds a path to template source and a
/// file describing the default values associated with
/// names used in the template
#[derive(Debug)]
pub struct Template {
    /// path to template source
    pub path: PathBuf,
}

impl Template {
    pub fn new<P>(path: P) -> Template
        where P: AsRef<Path>
    {
        Template { path: path.as_ref().to_path_buf() }
    }

    /// resolve context
    fn context(&self) -> Result<BTreeMap<String, String>> {
        let defaults_file = self.path.join(DEFAULTS);
        let map = defaults::from_file(defaults_file)?;
        let resolved = interact(&map)?;
        Ok(resolved)
    }

    /// Apply template
    pub fn apply<P>(&self, target: P) -> Result<()>
        where P: AsRef<Path>
    {
        let ctx = self.context()?;

        // apply handlebars processing
        let apply = |path: &Path, hbs: &mut Handlebars| -> Result<()> {

            // /tmp/download_dir/templates
            let scratchpath = &format!("{}{}",
                                       self.path.join(TEMPLATE_DIR).to_str().unwrap(),
                                       MAIN_SEPARATOR)[..];

            // path relatived based on scratch dir
            let localpath = path.to_str()
                .unwrap()
                .trim_left_matches(scratchpath);

            // eval path as template
            let evalpath = hbs.template_render(&localpath, &ctx)?;

            // rewritten path, based on target dir and eval path
            let targetpath = target.as_ref().join(evalpath);

            if path.is_dir() {
                fs::create_dir_all(targetpath)?
            } else {
                let mut file = File::open(path)?;
                let mut s = String::new();
                file.read_to_string(&mut s)?;
                if targetpath.exists() {
                    // open file for reading and writing
                    let mut file = OpenOptions::new().write(true)
                        .read(true)
                        .open(&targetpath)?;

                    // get the current content
                    let mut current_content = String::new();
                    file.read_to_string(&mut current_content)?;

                    // get the target content
                    let template_eval = hbs.template_render(&s, &ctx)?;

                    // if there's a diff prompt for change
                    if template_eval != current_content {
                        let keep = keep_current_content(current_content.as_ref(),
                                                        template_eval.as_ref(),
                                                        &targetpath)?;
                        if !keep {
                            // force truncation of current content
                            let mut file =
                                OpenOptions::new().write(true).truncate(true).open(targetpath)?;
                            file.write_all(template_eval.as_bytes())?;
                        }
                    }
                } else {
                    let mut file = File::create(targetpath)?;
                    hbs.template_renderw(&s, &ctx, &mut file)?;
                }
            }
            Ok(())
        };

        create_dir_all(target.as_ref())?;
        let mut hbs = bars();
        for entry in WalkDir::new(&self.path.join(TEMPLATE_DIR))
            .into_iter()
            .skip(1)
            .filter_map(|e| e.ok()) {
            debug!("applying {:?}", entry.path().display());
            apply(entry.path(), &mut hbs)?
        }
        Ok(())
    }
}

pub fn bars() -> Handlebars {
    let mut hbs = Handlebars::new();
    fn transform<F>(bars: &mut Handlebars, name: &str, f: F)
        where F: 'static + Fn(&str) -> String + Sync + Send
    {
        bars.register_helper(name,
                             Box::new(move |h: &Helper,
                                            _: &Handlebars,
                                            rc: &mut RenderContext|
                                            -> ::std::result::Result<(), RenderError> {
                                 let value = h.params().get(0).unwrap().value();
                                 rc.writer.write(f(value.as_string().unwrap()).as_bytes())?;
                                 Ok(())
                             }));
    }

    transform(&mut hbs, "upper", str::to_uppercase);
    transform(&mut hbs, "lower", str::to_lowercase);

    hbs
}

fn keep_current_content<P>(current: &str, new: &str, file: P) -> io::Result<bool>
    where P: AsRef<Path>
{
    let mut answer = String::new();
    println!("Conflicts exist with the current version of {}",
             file.as_ref().display());
    difference::print_diff(current, new, "\n");
    print!("Do you want to keep the previous version? [y/n]: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut answer)?;
    let trimmed = answer.trim().to_lowercase();
    Ok(trimmed.is_empty() || trimmed != String::from("n"))
}

/// prompt for a value defaulting to a given string when an answer is not available
fn prompt(name: &str, default: &String) -> io::Result<String> {
    let mut answer = String::new();
    print!("{} [{}]: ", name, default);
    io::stdout().flush()?;
    io::stdin().read_line(&mut answer)?;
    let trimmed = answer.trim();
    if trimmed.trim().is_empty() {
        Ok(default.to_owned())
    } else {
        Ok(trimmed.to_owned())
    }
}

/// given a set of defaults, attempt to interact with a user
/// to resolve the parameters that can not be inferred from env
fn interact(defaults: &defaults::Defaults) -> Result<BTreeMap<String, String>> {
    let mut resolved = BTreeMap::new();
    for (k, v) in defaults {
        let answer = match env::var(k) {
            Ok(v) => v,
            _ => prompt(k, v)?,
        };
        resolved.insert(k.clone(), answer);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;

    #[test]
    fn test_bars_upper() {
        let mut map = BTreeMap::new();
        map.insert("name".to_owned(), "porteurbars".to_owned());
        assert_eq!("Hello, PORTEURBARS",
                   bars().template_render("Hello, {{upper name}}", &map).unwrap());
    }

    #[test]
    fn test_bars_lower() {
        let mut map = BTreeMap::new();
        map.insert("name".to_owned(), "PORTEURBARS".to_owned());
        assert_eq!("Hello, porteurbars",
                   bars().template_render("Hello, {{lower name}}", &map).unwrap());
    }
}
