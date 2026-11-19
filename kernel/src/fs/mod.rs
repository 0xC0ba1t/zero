//! A small in-memory file system.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub enum Node {
    File(Vec<u8>),
    Directory(BTreeMap<String, Node>),
}

pub struct Filesystem {
    root: Node,
    cwd: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    InvalidPath,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: usize,
}

/// Turns `path` (relative to `cwd` unless it starts with `/`) into its
/// components, resolving `.` and `..`. `..` at the root stays at the root.
fn components(cwd: &str, path: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if !path.starts_with('/') {
        parts.extend(cwd.split('/').filter(|s| !s.is_empty()).map(String::from));
    }
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(String::from(name)),
        }
    }
    parts
}

fn join(parts: &[String]) -> String {
    if parts.is_empty() {
        return String::from("/");
    }
    let mut out = String::new();
    for part in parts {
        out.push('/');
        out.push_str(part);
    }
    out
}

impl Filesystem {
    pub fn new() -> Self {
        Self {
            root: Node::Directory(BTreeMap::new()),
            cwd: String::from("/"),
        }
    }

    pub fn get_cwd(&self) -> String {
        self.cwd.clone()
    }

    /// The absolute, normalized form of `path`.
    pub fn absolute(&self, path: &str) -> String {
        join(&components(&self.cwd, path))
    }

    pub fn set_cwd(&mut self, path: &str) -> Result<(), FsError> {
        let parts = components(&self.cwd, path);
        match self.node(&parts)? {
            Node::Directory(_) => {
                self.cwd = join(&parts);
                Ok(())
            }
            Node::File(_) => Err(FsError::NotADirectory),
        }
    }

    fn node(&self, parts: &[String]) -> Result<&Node, FsError> {
        let mut current = &self.root;
        for part in parts {
            match current {
                Node::Directory(dir) => current = dir.get(part).ok_or(FsError::NotFound)?,
                Node::File(_) => return Err(FsError::NotADirectory),
            }
        }
        Ok(current)
    }

    fn node_mut(&mut self, parts: &[String]) -> Result<&mut Node, FsError> {
        let mut current = &mut self.root;
        for part in parts {
            match current {
                Node::Directory(dir) => current = dir.get_mut(part).ok_or(FsError::NotFound)?,
                Node::File(_) => return Err(FsError::NotADirectory),
            }
        }
        Ok(current)
    }

    /// The directory that holds the last component of `path`, and that component.
    fn parent_mut(&mut self, path: &str) -> Result<(&mut BTreeMap<String, Node>, String), FsError> {
        let mut parts = components(&self.cwd, path);
        let name = parts.pop().ok_or(FsError::InvalidPath)?;
        match self.node_mut(&parts)? {
            Node::Directory(dir) => Ok((dir, name)),
            Node::File(_) => Err(FsError::NotADirectory),
        }
    }

    pub fn create_file(&mut self, path: &str) -> Result<(), FsError> {
        let (dir, name) = self.parent_mut(path)?;
        if dir.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        dir.insert(name, Node::File(Vec::new()));
        Ok(())
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        let (dir, name) = self.parent_mut(path)?;
        if dir.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        dir.insert(name, Node::Directory(BTreeMap::new()));
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        match self.node(&components(&self.cwd, path))? {
            Node::File(data) => Ok(data.clone()),
            Node::Directory(_) => Err(FsError::IsADirectory),
        }
    }

    pub fn write_file(&mut self, path: &str, data: Vec<u8>) -> Result<(), FsError> {
        let parts = components(&self.cwd, path);
        match self.node_mut(&parts) {
            Ok(Node::File(file)) => {
                *file = data;
                Ok(())
            }
            Ok(Node::Directory(_)) => Err(FsError::IsADirectory),
            // writing to a file that does not exist yet creates it
            Err(FsError::NotFound) => {
                let (dir, name) = self.parent_mut(path)?;
                dir.insert(name, Node::File(data));
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn delete(&mut self, path: &str) -> Result<(), FsError> {
        let (dir, name) = self.parent_mut(path)?;
        dir.remove(&name).map(|_| ()).ok_or(FsError::NotFound)
    }

    /// Names in a directory, sorted.
    pub fn list(&self, path: &str) -> Result<Vec<String>, FsError> {
        match self.node(&components(&self.cwd, path))? {
            Node::Directory(dir) => Ok(dir.keys().cloned().collect()),
            Node::File(_) => Err(FsError::NotADirectory),
        }
    }

    /// Entries of a directory with their kind and size, sorted by name.
    pub fn entries(&self, path: &str) -> Result<Vec<Entry>, FsError> {
        match self.node(&components(&self.cwd, path))? {
            Node::Directory(dir) => Ok(dir
                .iter()
                .map(|(name, node)| match node {
                    Node::File(data) => Entry { name: name.clone(), is_dir: false, size: data.len() },
                    Node::Directory(_) => Entry { name: name.clone(), is_dir: true, size: 0 },
                })
                .collect()),
            Node::File(_) => Err(FsError::NotADirectory),
        }
    }

    pub fn is_dir(&self, path: &str) -> bool {
        matches!(self.node(&components(&self.cwd, path)), Ok(Node::Directory(_)))
    }
}

pub static FILESYSTEM: Mutex<Option<Filesystem>> = Mutex::new(None);

#[test_case]
fn test_dot_dot_and_relative_paths() {
    let mut fs = Filesystem::new();
    fs.create_dir("/a").unwrap();
    fs.create_dir("/a/b").unwrap();
    fs.set_cwd("a/b").unwrap();
    assert_eq!(fs.get_cwd(), "/a/b");
    fs.set_cwd("..").unwrap();
    assert_eq!(fs.get_cwd(), "/a");
    fs.set_cwd("../..").unwrap();
    assert_eq!(fs.get_cwd(), "/");
    assert_eq!(fs.absolute("x/./y/../z"), "/x/z");
}

#[test_case]
fn test_files_round_trip() {
    let mut fs = Filesystem::new();
    fs.create_file("/f").unwrap();
    assert_eq!(fs.create_file("/f"), Err(FsError::AlreadyExists));
    fs.write_file("/f", b"hi".to_vec()).unwrap();
    assert_eq!(fs.read_file("f").unwrap(), b"hi");
    fs.write_file("/g", b"new".to_vec()).unwrap();
    assert_eq!(fs.list("/").unwrap(), ["f", "g"]);
    fs.delete("/f").unwrap();
    assert_eq!(fs.read_file("/f"), Err(FsError::NotFound));
    assert_eq!(fs.delete("/"), Err(FsError::InvalidPath));
}
