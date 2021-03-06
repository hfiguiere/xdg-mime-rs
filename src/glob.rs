use std::cmp::Ordering;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

use glob::Pattern;
use unicase::UniCase;

#[derive(Clone, PartialEq)]
pub enum GlobType {
    Literal(String),
    Simple(String),
    Full(Pattern),
}

impl Eq for GlobType {}

impl fmt::Debug for GlobType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GlobType::Literal(name) => write!(f, "Literal '{}'", name),
            GlobType::Simple(pattern) => write!(f, "Simple glob '*{}'", pattern),
            GlobType::Full(pattern) => write!(f, "Full glob '{}'", pattern),
        }
    }
}

fn determine_type<S: Into<String>>(glob: S) -> GlobType {
    let mut maybe_simple = false;
    let glob = glob.into();

    for (idx, ch) in glob.bytes().enumerate() {
        if idx == 0 && ch == b'*' {
            maybe_simple = true;
        } else if ch == b'\\' || ch == b'[' || ch == b'*' || ch == b'?' {
            return GlobType::Full(Pattern::new(&glob).unwrap());
        }
    }

    if maybe_simple {
        GlobType::Simple(glob[1..].to_string())
    } else {
        GlobType::Literal(glob)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Glob {
    glob: GlobType,
    weight: i32,
    case_sensitive: bool,
    mime_type: String,
}

impl fmt::Debug for Glob {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Glob: {:?} {:?} (weight: {}, cs: {})",
            self.glob, self.mime_type, self.weight, self.case_sensitive
        )
    }
}

impl Ord for Glob {
    fn cmp(&self, other: &Glob) -> Ordering {
        self.weight.cmp(&other.weight)
    }
}

impl PartialOrd for Glob {
    fn partial_cmp(&self, other: &Glob) -> Option<Ordering> {
        Some(self.weight.cmp(&other.weight))
    }
}

impl Glob {
    pub fn simple<S: Into<String>>(mime_type: S, glob: S) -> Glob {
        let mime_type = mime_type.into();
        let glob = glob.into();

        Glob {
            mime_type: mime_type,
            glob: determine_type(glob),
            weight: 50,
            case_sensitive: false,
        }
    }

    pub fn with_weight<S: Into<String>>(mime_type: S, glob: S, weight: i32) -> Glob {
        let mime_type = mime_type.into();
        let glob = glob.into();

        Glob {
            mime_type: mime_type,
            glob: determine_type(glob),
            weight: weight,
            case_sensitive: false,
        }
    }

    pub fn new<S: Into<String>>(mime_type: S, glob: S, weight: i32, cs: bool) -> Glob {
        let mime_type = mime_type.into();
        let glob = glob.into();

        Glob {
            mime_type: mime_type,
            glob: determine_type(glob),
            weight: weight,
            case_sensitive: cs,
        }
    }

    pub fn from_v1_string<S: Into<String>>(s: S) -> Option<Glob> {
        let s = s.into();

        if s.is_empty() || !s.contains(':') {
            return None;
        }

        let mut chunks = s.split(':');

        let mime_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        let glob = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        if mime_type.is_empty() || glob.is_empty() {
            return None;
        }

        // Consume the leftovers, if any
        if chunks.count() != 0 {
            return None;
        }

        Some(Glob {
            glob: determine_type(glob),
            mime_type: mime_type,
            weight: 50,
            case_sensitive: false,
        })
    }

    pub fn from_v2_string<S: Into<String>>(s: S) -> Option<Glob> {
        let s = s.into();

        if s.is_empty() || !s.contains(':') {
            return None;
        }

        let mut chunks = s.split(':');

        let weight = match chunks.next() {
            Some(v) => v.parse::<i32>().unwrap_or(-1),
            None => return None,
        };

        if weight < 0 {
            return None;
        }

        let mime_type = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        let glob = match chunks.next() {
            Some(v) => v.to_string(),
            None => return None,
        };

        if mime_type.is_empty() || glob.is_empty() {
            return None;
        }

        let case_sensitive = match chunks.next() {
            Some(v) => {
                if v == "cs" {
                    true
                } else {
                    return None;
                }
            }
            None => false,
        };

        // Consume the leftovers, if any
        if chunks.count() != 0 {
            return None;
        }

        Some(Glob {
            glob: determine_type(glob),
            weight: weight,
            case_sensitive: case_sensitive,
            mime_type: mime_type,
        })
    }

    fn compare(&self, file_name: &str) -> bool {
        match &self.glob {
            GlobType::Literal(s) => {
                let a = UniCase::new(s);
                let b = UniCase::new(file_name);

                return a == b;
            }
            GlobType::Simple(s) => {
                if file_name.ends_with(s) {
                    return true;
                }

                if !self.case_sensitive {
                    let lc_file_name = file_name.to_lowercase();
                    if lc_file_name.ends_with(s) {
                        return true;
                    }
                }
            }
            GlobType::Full(p) => {
                return p.matches(file_name);
            }
        }

        false
    }
}

pub fn read_globs_v1_from_file<P: AsRef<Path>>(file_name: P) -> Option<Vec<Glob>> {
    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut res = Vec::new();
    let file = BufReader::new(&f);
    for line in file.lines() {
        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Glob::from_v1_string(line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    Some(res)
}

pub fn read_globs_v2_from_file<P: AsRef<Path>>(file_name: P) -> Option<Vec<Glob>> {
    let f = match File::open(file_name) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut res = Vec::new();
    let file = BufReader::new(&f);
    for line in file.lines() {
        let line = line.unwrap();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Glob::from_v2_string(line) {
            Some(v) => res.push(v),
            None => continue,
        }
    }

    Some(res)
}

pub struct GlobMap {
    globs: Vec<Glob>,
}

impl GlobMap {
    pub fn new() -> GlobMap {
        GlobMap { globs: Vec::new() }
    }

    pub fn add_glob(&mut self, glob: Glob) {
        self.globs.push(glob);
    }

    pub fn add_globs(&mut self, globs: Vec<Glob>) {
        self.globs.extend(globs);
    }

    pub fn lookup_mime_type_for_file_name(&self, file_name: &str) -> Option<Vec<String>> {
        let mut matching_globs = Vec::new();

        for glob in &self.globs {
            if glob.compare(file_name) {
                matching_globs.push(glob.clone());
            }
        }

        if matching_globs.len() == 0 {
            return None;
        }

        matching_globs.sort();

        let mut res = Vec::new();
        for glob in matching_globs {
            res.push(glob.mime_type.clone());
        }

        Some(res)
    }
}

impl fmt::Debug for GlobMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut lines = String::new();
        for glob in &self.globs {
            lines.push_str(&format!("{:?}", glob));
            lines.push_str("\n");
        }

        write!(f, "Globs:\n{}", lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_type() {
        assert_eq!(
            determine_type("Makefile"),
            GlobType::Literal("Makefile".to_string())
        );

        assert_eq!(
            determine_type("*.gif"),
            GlobType::Simple(".gif".to_string())
        );

        assert_eq!(
            determine_type("x*.[ch]"),
            GlobType::Full(Pattern::new("x*.[ch]").unwrap())
        )
    }

    #[test]
    fn glob_v1_string() {
        assert_eq!(
            Glob::from_v1_string("text/rust:*.rs"),
            Some(Glob::simple("text/rust", "*.rs"))
        );
        assert_eq!(
            Glob::from_v1_string("text/rust:*.rs"),
            Some(Glob::new("text/rust", "*.rs", 50, false))
        );

        assert_eq!(Glob::from_v1_string(""), None);
        assert_eq!(Glob::from_v1_string("foo"), None);
        assert_eq!(Glob::from_v1_string("foo:"), None);
        assert_eq!(Glob::from_v1_string(":bar"), None);
        assert_eq!(Glob::from_v1_string(":"), None);
        assert_eq!(Glob::from_v1_string("foo:bar:baz"), None);
    }

    #[test]
    fn glob_v2_string() {
        assert_eq!(
            Glob::from_v2_string("80:text/rust:*.rs"),
            Some(Glob::with_weight("text/rust", "*.rs", 80))
        );
        assert_eq!(
            Glob::from_v2_string("80:text/rust:*.rs"),
            Some(Glob::new("text/rust", "*.rs", 80, false))
        );
        assert_eq!(
            Glob::from_v2_string("50:text/x-c++src:*.C:cs"),
            Some(Glob::new("text/x-c++src", "*.C", 50, true))
        );

        assert_eq!(Glob::from_v2_string(""), None);
        assert_eq!(Glob::from_v2_string("foo"), None);
        assert_eq!(Glob::from_v2_string("foo:"), None);
        assert_eq!(Glob::from_v2_string(":bar"), None);
        assert_eq!(Glob::from_v2_string(":"), None);
        assert_eq!(Glob::from_v2_string("foo:bar:baz"), None);
        assert_eq!(Glob::from_v2_string("foo:bar:baz:blah"), None);
    }

    #[test]
    fn compare() {
        // Literal
        let copying = Glob::new("text/x-copying", "copying", 50, false);
        assert_eq!(copying.compare(&"COPYING".to_string()), true);

        // Simple, case-insensitive
        let c_src = Glob::new("text/x-csrc", "*.c", 50, false);
        assert_eq!(c_src.compare(&"foo.c".to_string()), true);
        assert_eq!(c_src.compare(&"FOO.C".to_string()), true);

        // Simple, case-sensitive
        let cplusplus_src = Glob::new("text/x-c++src", "*.C", 50, true);
        assert_eq!(cplusplus_src.compare(&"foo.C".to_string()), true);
        assert_eq!(cplusplus_src.compare(&"foo.c".to_string()), false);
        assert_eq!(cplusplus_src.compare(&"foo.h".to_string()), false);

        // Full
        let video_x_anim = Glob::new("video/x-anim", "*.anim[1-9j]", 50, false);
        assert_eq!(video_x_anim.compare(&"foo.anim0".to_string()), false);
        assert_eq!(video_x_anim.compare(&"foo.anim8".to_string()), true);
        assert_eq!(video_x_anim.compare(&"foo.animk".to_string()), false);
        assert_eq!(video_x_anim.compare(&"foo.animj".to_string()), true);
    }
}
