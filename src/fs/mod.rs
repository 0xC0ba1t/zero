use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum Node {
    File(Vec<u8>),
    Directory(BTreeMap<String, Node>),
}

pub struct Filesystem {
    root: Node,
}

#[derive(Debug)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
}

impl Filesystem {
    pub fn new() -> Self {
        Self {
            root: Node::Directory(BTreeMap::new()),
        }
    }

    pub fn create_file(&mut self, path: &str) -> Result<(), FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for (i, part) in parts.iter().enumerate() {
            if let Node::Directory(ref mut dir) = current {
                if i == parts.len() - 1 {
                    if dir.contains_key(*part) {
                        return Err(FsError::AlreadyExists);
                    }
                    dir.insert(String::from(*part), Node::File(Vec::new()));
                    return Ok(());
                }
                current = dir.get_mut(*part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }
        Ok(())
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for (i, part) in parts.iter().enumerate() {
            if let Node::Directory(ref mut dir) = current {
                if i == parts.len() - 1 {
                    if dir.contains_key(*part) {
                        return Err(FsError::AlreadyExists);
                    }
                    dir.insert(String::from(*part), Node::Directory(BTreeMap::new()));
                    return Ok(());
                }
                current = dir.get_mut(*part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &self.root;

        for part in parts {
            if let Node::Directory(dir) = current {
                current = dir.get(part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }

        if let Node::File(data) = current {
            Ok(data.clone())
        } else {
            Err(FsError::IsADirectory)
        }
    }

    pub fn write_file(&mut self, path: &str, data: Vec<u8>) -> Result<(), FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for part in parts {
            if let Node::Directory(ref mut dir) = current {
                current = dir.get_mut(part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }

        if let Node::File(ref mut file_data) = current {
            *file_data = data;
            Ok(())
        } else {
            Err(FsError::IsADirectory)
        }
    }

    pub fn delete(&mut self, path: &str) -> Result<(), FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut self.root;

        for (i, part) in parts.iter().enumerate() {
            if let Node::Directory(ref mut dir) = current {
                if i == parts.len() - 1 {
                    dir.remove(*part).ok_or(FsError::NotFound)?;
                    return Ok(());
                }
                current = dir.get_mut(*part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }
        Ok(())
    }

    pub fn list(&self, path: &str) -> Result<Vec<String>, FsError> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &self.root;

        for part in parts {
            if let Node::Directory(dir) = current {
                current = dir.get(part).ok_or(FsError::NotFound)?;
            } else {
                return Err(FsError::NotADirectory);
            }
        }

        if let Node::Directory(dir) = current {
            Ok(dir.keys().cloned().collect())
        } else {
            Err(FsError::NotADirectory)
        }
    }
}

pub static FILESYSTEM: Mutex<Option<Filesystem>> = Mutex::new(None);
