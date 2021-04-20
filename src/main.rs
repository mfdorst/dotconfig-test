use anyhow::{bail, Result};
use clap::{load_yaml, App, ArgMatches};
use serde::Deserialize;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::os::unix;
use std::path::PathBuf;
use thiserror::Error;

fn main() -> Result<()> {
    if cfg!(windows) {
        bail!("Windows is not supported.");
    }
    let yaml = load_yaml!("cli.yml");
    let args = App::from_yaml(yaml).get_matches();
    let (config, dotfiles_dir) = get_config(&args)?;

    // Symlink each file listed in config.links
    for Link { src, dest } in config.links {
        if let Err(e) = link(&src, &dest, &dotfiles_dir) {
            println!("{}", link_error_msg(e)?);
        }
    }
    Ok(())
}

macro_rules! link_error {
    ($fmt:expr, $($arg:tt)*) => { anyhow::Error::from(Error::LinkError(format!($fmt, $($arg)*))) }
}

macro_rules! link_err {
    ($fmt:expr, $($arg:tt)*) => { Err(link_error!($fmt, $($arg)*)) }
}

fn link(src: &str, dest: &str, dotfiles_dir: &PathBuf) -> Result<()> {
    let src = get_src_path(dotfiles_dir, src)?;
    let dest = expand_to_path_buf(&dest)?;
    let dest_parent = match dest.parent() {
        Some(path) => path,
        None => {
            // This should only happen if dest is '/'.
            return link_err!("Cannot link to '{}'. Skipping...", dest.display());
        }
    };
    let dest_parent = match fs::canonicalize(&dest_parent) {
        Ok(path) => path,
        Err(_) => {
            return link_err!(
                "Cannot link to '{}' because its parent directory does not exist. Skipping...",
                dest.display()
            )
        }
    };
    let dest_file_name = match dest.file_name() {
        Some(file_name) => file_name,
        None => {
            return link_err!("Invalid destination path '{}'. Skipping...", dest.display());
        }
    };
    let dest = dest_parent.join(dest_file_name);

    if dest.exists() {
        let mut backup_file = dest_file_name.to_owned();
        backup_file.push(
            chrono::Local::now()
                .format("-backup-%Y-%m-%d-%H-%M-%S")
                .to_string(),
        );
        let backup = dest_parent.join(backup_file);
        print!(
            "The path '{}' already exists. Backing up to '{}'...",
            dest.display(),
            backup.display()
        );
        match fs::rename(&dest, backup) {
            Ok(()) => println!("done."),
            Err(_) => println!("\nBackup failed. Skipping..."),
        }
    }
    print!("Linking {} -> {}...", src.display(), dest.display());
    match unix::fs::symlink(&src, &dest) {
        Ok(()) => println!("done."),
        Err(_) => {
            return link_err!(
                "\nFailed to symlink {} -> {}. Skipping...",
                src.display(),
                dest.display()
            )
        }
    };
    Ok(())
}

fn get_src_path(dotfiles_dir: &PathBuf, src: &str) -> Result<PathBuf> {
    let src = dotfiles_dir.join(src);
    let src = fs::canonicalize(&src)
        .map_err(|_| link_error!("Path '{}' does not exist. Skipping...", src.display()))?;
    Ok(src)
}

fn get_config(args: &ArgMatches) -> Result<(Config, PathBuf)> {
    // It's okay to unwrap here because dir has a default argument and will never be None.
    let dotfiles_dir = expand_to_path_buf(args.value_of("dir").unwrap())?;
    // It's okay to unwrap here because config has a default argument and will never be None
    let config_rel_path = expand_to_path_buf(args.value_of("config").unwrap())?;
    let config_full_path = dotfiles_dir.join(config_rel_path);

    if !dotfiles_dir.exists() {
        return Err(Error::MissingDotfilesDir(dotfiles_dir).into());
    }
    if !config_full_path.exists() {
        return Err(Error::MissingConfigFile(config_full_path).into());
    }
    let reader = BufReader::new(File::open(config_full_path)?);
    let config: Config = serde_yaml::from_reader(reader)?;
    Ok((config, dotfiles_dir))
}

fn expand_to_path_buf(path: &str) -> Result<PathBuf> {
    Ok(shellexpand::full(path)?.to_string().into())
}

fn link_error_msg(error: anyhow::Error) -> Result<String> {
    for cause in error.chain() {
        if let Some(Error::LinkError(explanation)) = cause.downcast_ref::<Error>() {
            return Ok(explanation.clone());
        }
    }
    Err(error)
}

#[derive(Deserialize, Debug)]
struct Config {
    links: Vec<Link>,
}

#[derive(Deserialize, Debug)]
struct Link {
    src: String,
    dest: String,
}

#[derive(Error, Debug)]
enum Error {
    #[error("Missing dotfiles directory ({0}).")]
    MissingDotfilesDir(PathBuf),
    #[error("Missing config file ({0}).")]
    MissingConfigFile(PathBuf),
    #[error("{0}")]
    LinkError(String),
}
