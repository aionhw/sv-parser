//! SystemVerilog preprocessor (IEEE 1800-2017 §22)
//!
//! Handles `define, `ifdef/`ifndef/`else/`endif, `include, `undef, etc.
//! This is a simplified preprocessor suitable for parsing purposes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: String,
    pub params: Option<Vec<String>>,
    pub body: String,
}

pub struct Preprocessor {
    defines: HashMap<String, MacroDef>,
    /// Directories to search for `include files (in order).
    /// The directory of the current source file is always searched first.
    include_dirs: Vec<PathBuf>,
    /// Current include depth (to prevent infinite recursion).
    include_depth: usize,
}

const MAX_INCLUDE_DEPTH: usize = 32;

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            defines: HashMap::new(),
            include_dirs: Vec::new(),
            include_depth: 0,
        }
    }

    /// Set include search directories.
    pub fn set_include_dirs(&mut self, dirs: Vec<PathBuf>) {
        self.include_dirs = dirs;
    }

    /// Add an include search directory.
    pub fn add_include_dir(&mut self, dir: PathBuf) {
        if !self.include_dirs.contains(&dir) {
            self.include_dirs.push(dir);
        }
    }

    pub fn with_defines(defines: HashMap<String, String>) -> Self {
        let mut pp = Self::new();
        for (k, v) in defines {
            pp.defines.insert(k.clone(), MacroDef {
                name: k,
                params: None,
                body: v,
            });
        }
        pp
    }

    pub fn define(&mut self, name: String, value: MacroDef) {
        self.defines.insert(name, value);
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.defines.contains_key(name)
    }

    /// Preprocess source text, resolving `include directives relative to `source_path`.
    /// If `source_path` is None, `include directives that require file I/O are skipped.
    pub fn preprocess_file(&mut self, source: &str, source_path: Option<&Path>) -> String {
        self.preprocess_inner(source, source_path)
    }

    /// Simple preprocessing pass (no file context — `include lines are skipped).
    pub fn preprocess(&mut self, source: &str) -> String {
        self.preprocess_inner(source, None)
    }

    fn preprocess_inner(&mut self, source: &str, source_path: Option<&Path>) -> String {
        let mut output = String::with_capacity(source.len());
        let mut lines = source.lines().peekable();
        let mut ifdef_stack: Vec<bool> = Vec::new(); // true = active

        // Directory of the current source file (for relative `include resolution)
        let source_dir = source_path.and_then(|p| p.parent().map(|d| d.to_path_buf()));

        while let Some(line) = lines.next() {
            let trimmed = line.trim();

            // Strip (* ... *) attributes (IEEE 1800-2017 §5.12)
            // These are synthesis/tool directives that don't affect simulation
            if trimmed.starts_with("(*") && trimmed.ends_with("*)") {
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`define") {
                if ifdef_stack.iter().all(|&b| b) {
                    self.parse_define(trimmed);
                }
                // Don't output `define lines
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`undef") {
                if ifdef_stack.iter().all(|&b| b) {
                    let name = trimmed[6..].trim().to_string();
                    self.defines.remove(&name);
                }
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`ifdef") {
                let name = trimmed[6..].trim();
                // Strip trailing // comments from ifdef macro name
                let name = name.split_whitespace().next().unwrap_or(name);
                ifdef_stack.push(self.is_defined(name));
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`ifndef") {
                let name = trimmed[7..].trim();
                let name = name.split_whitespace().next().unwrap_or(name);
                ifdef_stack.push(!self.is_defined(name));
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`else") {
                if let Some(last) = ifdef_stack.last_mut() {
                    *last = !*last;
                }
                output.push('\n');
                continue;
            }

            if trimmed.starts_with("`endif") {
                ifdef_stack.pop();
                output.push('\n');
                continue;
            }

            // Skip inactive blocks
            if !ifdef_stack.iter().all(|&b| b) {
                output.push('\n');
                continue;
            }

            // Handle `include — read and recursively preprocess the included file
            if trimmed.starts_with("`include") {
                if let Some(inc_file) = Self::parse_include_path(trimmed) {
                    if self.include_depth < MAX_INCLUDE_DEPTH {
                        if let Some(resolved) = self.resolve_include(&inc_file, source_dir.as_deref()) {
                            match std::fs::read_to_string(&resolved) {
                                Ok(contents) => {
                                    self.include_depth += 1;
                                    let included = self.preprocess_inner(&contents, Some(&resolved));
                                    self.include_depth -= 1;
                                    output.push_str(&included);
                                    // Don't push extra newline — included content has its own
                                    continue;
                                }
                                Err(e) => {
                                    eprintln!("[PP] warning: cannot read `include file '{}': {}", resolved.display(), e);
                                }
                            }
                        } else {
                            eprintln!("[PP] warning: cannot find `include file '{}'", inc_file);
                        }
                    } else {
                        eprintln!("[PP] warning: `include depth limit ({}) exceeded for '{}'", MAX_INCLUDE_DEPTH, inc_file);
                    }
                }
                output.push('\n');
                continue;
            }

            // Skip `timescale and other compiler directives
            // that don't affect simulation semantics
            if trimmed.starts_with("`timescale")
                || trimmed.starts_with("`default_nettype")
                || trimmed.starts_with("`celldefine") || trimmed.starts_with("`endcelldefine")
                || trimmed.starts_with("`resetall")
                || trimmed.starts_with("`nounconnected_drive") || trimmed.starts_with("`unconnected_drive")
                || trimmed.starts_with("`pragma")
                || trimmed.starts_with("`begin_keywords") || trimmed.starts_with("`end_keywords")
                || trimmed.starts_with("`line")
            {
                output.push('\n');
                continue;
            }

            // Expand macros in the line
            let expanded = self.expand_macros(line);
            // Strip inline (* ... *) attributes
            let expanded = Self::strip_attributes(&expanded);
            output.push_str(&expanded);
            output.push('\n');
        }

        output
    }

    /// Extract the filename from an `include directive.
    /// Handles both `include "file.v" and `include <file.v> forms.
    fn parse_include_path(line: &str) -> Option<String> {
        let rest = line.strip_prefix("`include")?.trim();
        if rest.starts_with('"') {
            // `include "filename"
            let end = rest[1..].find('"')?;
            Some(rest[1..1 + end].to_string())
        } else if rest.starts_with('<') {
            // `include <filename>
            let end = rest[1..].find('>')?;
            Some(rest[1..1 + end].to_string())
        } else {
            None
        }
    }

    /// Resolve an `include filename to an absolute path by searching:
    /// 1. The directory of the currently-processed source file
    /// 2. Each directory in include_dirs (in order)
    fn resolve_include(&self, filename: &str, source_dir: Option<&Path>) -> Option<PathBuf> {
        let inc_path = Path::new(filename);

        // If the include path is absolute, use it directly
        if inc_path.is_absolute() {
            if inc_path.exists() {
                return Some(inc_path.to_path_buf());
            }
            return None;
        }

        // Search relative to the current source file's directory first
        if let Some(dir) = source_dir {
            let candidate = dir.join(inc_path);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        // Search include directories
        for dir in &self.include_dirs {
            let candidate = dir.join(inc_path);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    fn parse_define(&mut self, line: &str) {
        let rest = line[7..].trim(); // after `define
        // Find name
        let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
        let name = rest[..name_end].to_string();
        let after_name = &rest[name_end..];
        
        // Check for parameterized macro: `define NAME(param1, param2) body
        let (params, body) = if after_name.starts_with('(') {
            // Find closing paren
            if let Some(close) = after_name.find(')') {
                let param_str = &after_name[1..close];
                let params: Vec<String> = param_str.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let body = after_name[close + 1..].trim().to_string();
                (Some(params), body)
            } else {
                (None, after_name.trim().to_string())
            }
        } else {
            (None, after_name.trim().to_string())
        };
        
        if !name.is_empty() {
            self.defines.insert(name.clone(), MacroDef {
                name,
                params,
                body,
            });
        }
    }

    fn expand_macros(&self, line: &str) -> String {
        let mut result = self.expand_macros_once(line);
        // Recursively expand up to 16 times to handle nested macros
        for _ in 0..16 {
            if !result.contains('`') { break; }
            let next = self.expand_macros_once(&result);
            if next == result { break; }
            result = next;
        }
        result
    }

    fn expand_macros_once(&self, line: &str) -> String {
        let mut result = String::with_capacity(line.len());
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'`' {
                i += 1;
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let macro_name = &line[start..i];
                if let Some(def) = self.defines.get(macro_name) {
                    if def.params.is_some() && i < bytes.len() && bytes[i] == b'(' {
                        // Parameterized macro: find arguments
                        let args = Self::extract_macro_args(line, &mut i);
                        let params = def.params.as_ref().unwrap();
                        let mut body = def.body.clone();
                        for (pi, pname) in params.iter().enumerate() {
                            if let Some(arg) = args.get(pi) {
                                body = body.replace(pname, arg);
                            }
                        }
                        result.push_str(&body);
                    } else {
                        result.push_str(&def.body);
                    }
                } else {
                    result.push('`');
                    result.push_str(macro_name);
                }
            } else {
                result.push(line[i..].chars().next().unwrap());
                i += 1;
            }
        }
        result
    }
}

impl Default for Preprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Preprocessor {
    /// Strip (* ... *) Verilog attributes from a line
    /// Extract parenthesized macro arguments, handling nested parens.
    /// `i` should point at the opening '('. After return, `i` is past the closing ')'.
    fn extract_macro_args(line: &str, i: &mut usize) -> Vec<String> {
        let bytes = line.as_bytes();
        *i += 1; // skip '('
        let mut args = Vec::new();
        let mut depth = 1;
        let mut arg_start = *i;
        while *i < bytes.len() && depth > 0 {
            match bytes[*i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let arg = line[arg_start..*i].trim().to_string();
                        if !arg.is_empty() || !args.is_empty() {
                            args.push(arg);
                        }
                        *i += 1; // skip ')'
                        return args;
                    }
                }
                b',' if depth == 1 => {
                    args.push(line[arg_start..*i].trim().to_string());
                    arg_start = *i + 1;
                }
                _ => {}
            }
            *i += 1;
        }
        args
    }

    fn strip_attributes(line: &str) -> String {
        let mut result = String::with_capacity(line.len());
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'(' && bytes[i + 1] == b'*' {
                // Check this isn't inside a string
                // Find matching *)
                let mut j = i + 2;
                while j + 1 < bytes.len() {
                    if bytes[j] == b'*' && bytes[j + 1] == b')' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                if j <= bytes.len() {
                    // Replace attribute with space to preserve spacing
                    result.push(' ');
                    i = j;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        result
    }
}
