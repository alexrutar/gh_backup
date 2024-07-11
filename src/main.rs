pub mod date;
pub mod entry;

use std::fs;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use chrono::{DateTime, FixedOffset, Local};
use clap::Parser;
use color_eyre::eyre::Result;
use serde::Deserializer;
use tokio::process::Command;
use tokio::task::JoinSet;

use date::LastUpdated;
use entry::DeserializeUserRepos;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The maximum number of repositories per account to download.
    #[arg(short, long, value_name = "NUM", default_value_t = 1000)]
    max: usize,

    /// The number of repositories to update this run.
    #[arg(short, long, value_name = "NUM", default_value_t = 20)]
    limit: usize,

    /// The list of users to backup.
    users: Vec<String>,
}

pub struct BackupFile<'a> {
    path: &'a str,
}

impl<'a> BackupFile<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn read(&self) -> Result<DateTime<FixedOffset>> {
        Ok(match fs::read(self.path) {
            Ok(bytes) => DateTime::<FixedOffset>::parse_from_rfc3339(std::str::from_utf8(&bytes)?)?,
            Err(_) => DateTime::UNIX_EPOCH.into(),
        })
    }

    pub fn write(&self, time: DateTime<FixedOffset>) -> Result<(), std::io::Error> {
        fs::write(self.path, time.to_rfc3339())
    }
}

/// Update the repository, recording the update time and whether or not the update was successful.
pub async fn git_update(
    repo: String,
    backup_path: &'static Path,
) -> Result<(String, DateTime<FixedOffset>, ExitStatus), std::io::Error> {
    let execute_time = Local::now().into();

    let status = Command::new("git")
        .args(["-C", &repo, "pull"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(&backup_path)
        .status()
        .await?;

    let status = if !status.success() {
        Command::new("gh")
            .args(["repo", "clone", &repo, &repo])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .current_dir(&backup_path)
            .status()
            .await?
    } else {
        status
    };

    Ok((repo, execute_time, status))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let Cli { limit, max, users } = Cli::parse();
    let max_string = max.to_string();

    let xdg_dirs = xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))?;
    let last_updated_path = xdg_dirs.place_data_file("last_updated.json")?;
    let backup_path: &'static Path = Box::leak(Box::new(xdg_dirs.create_data_directory("backup")?));

    // read last updated
    let mut last_updated = LastUpdated::read_from_file(&last_updated_path)?;

    // initialize futures
    let mut entry_set = JoinSet::new();
    for user in users {
        let output = Command::new("gh")
            .args([
                "repo",
                "ls",
                &user,
                "--limit",
                &max_string,
                "--json",
                "nameWithOwner",
                "--json",
                "updatedAt",
            ])
            .current_dir(&backup_path)
            .output();

        entry_set.spawn(output);
    }

    // join futures to get all entries which require updating
    let mut to_update = Vec::new();
    while let Some(output) = entry_set.join_next().await {
        let output = output??.stdout;

        let mut json_de = serde_json::Deserializer::from_slice(&output);
        json_de.deserialize_seq(DeserializeUserRepos::new(&last_updated, &mut to_update))?;
    }

    to_update.truncate(limit);

    // update the corresponding entries
    let mut update_set = JoinSet::new();
    for entry in to_update.drain(..) {
        let cmd = git_update(entry.repo, backup_path);
        update_set.spawn(cmd);
    }

    // record the corresponding updates
    while let Some(res) = update_set.join_next().await {
        let (repo, execute_time, status) = res??;
        if status.success() {
            last_updated.update(repo, execute_time);
        }
    }

    last_updated.write_to_file(last_updated_path)?;

    Ok(())
}
