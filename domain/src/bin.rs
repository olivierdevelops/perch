use crate::program::Requirements;

/// Basename of a bin token (strips any path prefix).
fn bin_base(s: &str) -> &str {
    match s.rfind(['/', '\\']) {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

impl Requirements {
    /// Whether a bin token (first token of an `exec` / shell command, post-
    /// interpolation) is permitted by the manifest: equals an alias, equals a
    /// Name exactly, or shares its basename.
    pub fn bin_allowed(&self, token: &str) -> bool {
        if token.is_empty() {
            return true;
        }
        let base = bin_base(token);
        self.bins.iter().any(|b| {
            (!b.alias.is_empty() && token == b.alias)
                || token == b.name
                || base == bin_base(&b.name)
        })
    }

    /// Maps a bin token to the executable to spawn: an alias resolves to its
    /// declared name; everything else passes through unchanged.
    pub fn resolve_bin<'a>(&'a self, token: &'a str) -> &'a str {
        self.bins
            .iter()
            .find(|b| !b.alias.is_empty() && token == b.alias)
            .map(|b| b.name.as_str())
            .unwrap_or(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::BinReq;

    fn req(name: &str, alias: &str) -> BinReq {
        BinReq { name: name.into(), alias: alias.into(), ..Default::default() }
    }

    #[test]
    fn bin_allowed() {
        let r = Requirements { bins: vec![req("go", ""), req("./bins/tool.exe", "tool")], ..Default::default() };
        for (token, want) in [
            ("go", true),
            ("/usr/local/go/bin/go", true),
            ("./bins/tool.exe", true),
            ("tool", true),
            ("tool.exe", true),
            ("docker", false),
            ("", true),
        ] {
            assert_eq!(r.bin_allowed(token), want, "{token:?}");
        }
    }

    #[test]
    fn resolve_bin() {
        let r = Requirements { bins: vec![req("./bins/tool.exe", "tool")], ..Default::default() };
        assert_eq!(r.resolve_bin("tool"), "./bins/tool.exe");
        assert_eq!(r.resolve_bin("go"), "go");
    }
}
