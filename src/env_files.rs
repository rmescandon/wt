use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn find_env_files(repository_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![repository_root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read directory {}", directory.display()))?
        {
            let entry = entry.with_context(|| format!("read entry in {}", directory.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("read file type for {}", path.display()))?;

            if file_type.is_dir() {
                if entry.file_name() != OsStr::new(".git") {
                    pending.push(path);
                }
            } else if file_type.is_file() && is_env_file(&entry.file_name()) {
                files.push(
                    path.strip_prefix(repository_root)
                        .context("found .env file outside repository root")?
                        .to_path_buf(),
                );
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_env_file(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let is_env = name == ".env" || name.starts_with(".env.");
    let is_template = [".example", ".sample", ".template"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    is_env && !is_template
}

pub fn copy_env_files(
    repository_root: &Path,
    destination_root: &Path,
    files: &[PathBuf],
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let destination_metadata = fs::symlink_metadata(destination_root)
        .with_context(|| format!("inspect worktree {}", destination_root.display()))?;
    if !destination_metadata.file_type().is_dir() {
        anyhow::bail!(
            "worktree destination is not a directory: {}",
            destination_root.display()
        );
    }

    let mut copies = Vec::with_capacity(files.len());
    for relative_path in files {
        validate_relative_path(relative_path)?;
        let source = repository_root.join(relative_path);
        let source_metadata = fs::symlink_metadata(&source)
            .with_context(|| format!("inspect .env file {}", source.display()))?;
        if !source_metadata.file_type().is_file() {
            anyhow::bail!(".env source is not a regular file: {}", source.display());
        }

        let destination = destination_root.join(relative_path);
        validate_destination_path(destination_root, relative_path)?;
        copies.push((source, destination, source_metadata.permissions()));
    }

    let mut copied = Vec::new();
    let mut created_directories = Vec::new();
    for (source, destination, permissions) in copies {
        let result = (|| -> Result<()> {
            let relative_parent = destination
                .strip_prefix(destination_root)
                .context("destination path escaped worktree")?
                .parent()
                .unwrap_or_else(|| Path::new(""));
            create_parent_directories(destination_root, relative_parent, &mut created_directories)?;

            let mut source_file = File::open(&source)
                .with_context(|| format!("open .env file {}", source.display()))?;
            let mut destination_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .with_context(|| format!("create .env file {}", destination.display()))?;
            copied.push(destination.clone());
            io::copy(&mut source_file, &mut destination_file)
                .with_context(|| format!("copy .env file to {}", destination.display()))?;
            destination_file
                .set_permissions(permissions)
                .with_context(|| format!("set permissions for {}", destination.display()))?;
            Ok(())
        })();

        if let Err(error) = result {
            rollback(&copied, &created_directories);
            return Err(error);
        }
    }

    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        anyhow::bail!("invalid relative .env path: {}", path.display());
    }
    Ok(())
}

fn validate_destination_path(destination_root: &Path, relative_path: &Path) -> Result<()> {
    let components = relative_path.components().collect::<Vec<_>>();
    let mut current = destination_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(_) if index + 1 == components.len() => {
                anyhow::bail!("worktree already contains {}", current.display());
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                anyhow::bail!("worktree path is not a directory: {}", current.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error).with_context(|| format!("inspect {}", current.display()));
            }
        }
    }
    Ok(())
}

fn create_parent_directories(
    destination_root: &Path,
    relative_parent: &Path,
    created_directories: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut current = destination_root.to_path_buf();
    for component in relative_parent.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("invalid .env parent path: {}", relative_parent.display());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => anyhow::bail!("worktree path is not a directory: {}", current.display()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .with_context(|| format!("create directory {}", current.display()))?;
                created_directories.push(current.clone());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("inspect {}", current.display()));
            }
        }
    }
    Ok(())
}

fn rollback(copied: &[PathBuf], created_directories: &[PathBuf]) {
    for path in copied.iter().rev() {
        let _ = fs::remove_file(path);
    }
    for path in created_directories.iter().rev() {
        let _ = fs::remove_dir(path);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after Unix epoch")
                .as_nanos();
            let mut attempt = 0;
            let path = loop {
                let candidate = std::env::temp_dir().join(format!(
                    "wt-env-files-test-{}-{unique}-{attempt}",
                    std::process::id()
                ));
                match fs::create_dir(&candidate) {
                    Ok(()) => break candidate,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => attempt += 1,
                    Err(error) => panic!("create test directory: {error}"),
                }
            };
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(root: &Path, path: &str, contents: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("test file parent"))
            .expect("create test parent directory");
        fs::write(path, contents).expect("write test file");
    }

    #[test]
    fn finds_nested_env_files_but_skips_templates_and_git_directory() {
        let directory = TestDirectory::new();
        write_file(&directory.0, ".env", "root secret");
        write_file(&directory.0, "service/.env.local", "local secret");
        write_file(&directory.0, "service/.env.production", "production secret");
        write_file(&directory.0, ".env.example", "example");
        write_file(&directory.0, "service/.env.sample", "sample");
        write_file(&directory.0, "service/.env.test.template", "template");
        write_file(&directory.0, ".git/.env", "git internal");

        let files = super::find_env_files(&directory.0).expect("find env files");

        assert_eq!(
            files,
            [
                PathBuf::from(".env"),
                PathBuf::from("service/.env.local"),
                PathBuf::from("service/.env.production"),
            ]
        );
    }

    #[test]
    fn copies_all_env_files_preserving_relative_paths_and_permissions() {
        let directory = TestDirectory::new();
        let repository = directory.0.join("repository");
        let destination = directory.0.join("worktree");
        fs::create_dir_all(&repository).expect("create repository");
        fs::create_dir_all(&destination).expect("create worktree");
        write_file(&repository, ".env", "root secret");
        write_file(&repository, "service/.env.local", "local secret");
        let files = super::find_env_files(&repository).expect("find env files");

        super::copy_env_files(&repository, &destination, &files).expect("copy env files");

        assert_eq!(
            fs::read_to_string(destination.join(".env")).unwrap(),
            "root secret"
        );
        assert_eq!(
            fs::read_to_string(destination.join("service/.env.local")).unwrap(),
            "local secret"
        );
        assert_eq!(
            fs::metadata(repository.join(".env")).unwrap().permissions(),
            fs::metadata(destination.join(".env"))
                .unwrap()
                .permissions()
        );
    }

    #[test]
    fn destination_conflict_prevents_every_env_file_from_being_copied() {
        let directory = TestDirectory::new();
        let repository = directory.0.join("repository");
        let destination = directory.0.join("worktree");
        fs::create_dir_all(&repository).expect("create repository");
        fs::create_dir_all(destination.join("service")).expect("create worktree");
        write_file(&repository, ".env", "root secret");
        write_file(&repository, "service/.env.local", "new secret");
        write_file(&destination, "service/.env.local", "existing secret");
        let files = super::find_env_files(&repository).expect("find env files");

        let error = super::copy_env_files(&repository, &destination, &files)
            .expect_err("reject existing destination file");

        assert!(format!("{error:#}").contains("worktree already contains"));
        assert!(!destination.join(".env").exists());
        assert_eq!(
            fs::read_to_string(destination.join("service/.env.local")).unwrap(),
            "existing secret"
        );
    }
}
