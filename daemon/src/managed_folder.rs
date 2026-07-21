use std::fs::{self, DirEntry, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// A canonical, read-only capability for one configured Scanned folder.
#[derive(Debug, Clone)]
pub struct ManagedFolder {
    id: u32,
    root: PathBuf,
}

#[derive(Debug)]
pub struct Blake3Hash {
    pub hash: Vec<u8>,
    pub open_duration: Duration,
    pub read_duration: Duration,
    pub update_duration: Duration,
}

impl ManagedFolder {
    pub fn open(id: u32, root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root.as_ref()).with_context(|| {
            format!(
                "unable to access Scanned folder {}",
                root.as_ref().display()
            )
        })?;
        if !root.is_dir() {
            bail!("Scanned folder {} is not a directory", root.display());
        }
        Ok(Self { id, root })
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    pub fn canonicalize(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = fs::canonicalize(path.as_ref())?;
        if !path.starts_with(&self.root) {
            bail!(
                "path {} escapes Scanned folder {}",
                path.display(),
                self.root.display()
            );
        }
        Ok(path)
    }

    pub fn resolve_relative(&self, relative: &Path) -> Result<PathBuf> {
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            bail!("Scanned folder paths must be relative and cannot traverse directories");
        }
        self.canonicalize(self.root.join(relative))
    }

    pub fn read_dir(&self, directory: &Path) -> Result<Vec<DirEntry>> {
        let directory = self.canonicalize(directory)?;
        Ok(fs::read_dir(directory)?.filter_map(Result::ok).collect())
    }

    pub fn metadata(&self, path: &Path) -> Result<Metadata> {
        Ok(fs::metadata(self.canonicalize(path)?)?)
    }

    pub fn read(&self, path: &Path, limit: Option<u64>) -> Result<Vec<u8>> {
        let path = self.canonicalize(path)?;
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        match limit {
            Some(limit) => file.take(limit).read_to_end(&mut bytes)?,
            None => file.read_to_end(&mut bytes)?,
        };
        Ok(bytes)
    }

    #[allow(dead_code)]
    pub fn blake3(&self, path: &Path) -> Result<Vec<u8>> {
        Ok(self.blake3_timed(path)?.hash)
    }

    pub fn blake3_timed(&self, path: &Path) -> Result<Blake3Hash> {
        let path = self.canonicalize(path)?;
        let open_started = Instant::now();
        let mut file = fs::File::open(path)?;
        let open_duration = open_started.elapsed();
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut read_duration = Duration::ZERO;
        let mut update_duration = Duration::ZERO;
        loop {
            let read_started = Instant::now();
            let read = file.read(&mut buffer)?;
            read_duration += read_started.elapsed();
            if read == 0 {
                break;
            }
            let update_started = Instant::now();
            hasher.update(&buffer[..read]);
            update_duration += update_started.elapsed();
        }
        Ok(Blake3Hash {
            hash: hasher.finalize().as_bytes().to_vec(),
            open_duration,
            read_duration,
            update_duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        let root = std::env::temp_dir().join(format!("puppydrive-folder-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let folder = ManagedFolder::open(1, &root).unwrap();
        assert!(folder.resolve_relative(Path::new("../outside")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("puppydrive-folder-{}", uuid::Uuid::new_v4()));
        let outside =
            std::env::temp_dir().join(format!("puppydrive-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("private.jpg"), b"private").unwrap();
        symlink(outside.join("private.jpg"), root.join("escape.jpg")).unwrap();

        let folder = ManagedFolder::open(1, &root).unwrap();
        assert!(folder.canonicalize(root.join("escape.jpg")).is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
