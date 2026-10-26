//! Automatic workspaces: a pure, bounded lattice over salted hashes of the
//! accessibility document's ancestor folders, plus web hosts. No filesystem
//! access happens here; observed paths and hosts are transient inputs, and only
//! salted keys, day stamps and counts are kept. A folder name or host becomes a
//! label only when its workspace turns visible.
use crate::{AppCategory, ProjectSelection, ProjectSource, WorkspaceContext};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;
pub const MAX_WORKSPACE_NODES: usize = 384;
pub const MAX_WORKSPACES: usize = 64;
/// A latent workspace becomes visible after this many dictations…
pub const VISIBLE_DICTATIONS: u32 = 5;
/// …spread over at least this many distinct days.
pub const VISIBLE_DAYS: usize = 2;
/// A folder becomes a (latent) root once this many distinct documents anchor
/// there, seen on at least `ROOT_DAYS` days.
pub const ROOT_DOCUMENTS: usize = 2;
pub const ROOT_DAYS: usize = 2;
const MAX_LEAVES: usize = 4;
const MAX_DAYS: usize = 8;
const MAX_CHAIN: usize = 8;
const MAX_LABEL_BYTES: usize = 64;
const KEY_HEX: usize = 32;
const LEAF_HEX: usize = 16;

/// Receiving-app category from a fixed bundle-identifier table. Never inferred
/// from window titles, text or URLs.
pub fn app_category(bundle_id: &str) -> AppCategory {
    let id = bundle_id.trim();
    let lower = id.to_ascii_lowercase();
    let exact = |list: &[&str]| {
        list.iter()
            .any(|candidate| lower == candidate.to_ascii_lowercase())
    };
    if exact(&[
        "com.apple.dt.Xcode",
        "com.microsoft.VSCode",
        "com.microsoft.VSCodeInsiders",
        "com.vscodium",
        "com.todesktop.230313mzl4w4u92", // Cursor
        "com.exafunction.windsurf",
        "dev.zed.Zed",
        "dev.zed.Zed-Preview",
        "com.google.android.studio",
        "com.sublimetext.4",
        "com.sublimetext.3",
        "com.panic.Nova",
        "com.barebones.bbedit",
        "org.vim.MacVim",
        "org.gnu.Emacs",
    ]) || lower.starts_with("com.jetbrains.")
    {
        return AppCategory::Code;
    }
    if exact(&[
        "com.apple.Terminal",
        "com.googlecode.iterm2",
        "com.mitchellh.ghostty",
        "dev.warp.Warp-Stable",
        "dev.warp.Warp",
        "net.kovidgoyal.kitty",
        "io.alacritty",
        "org.alacritty",
        "com.github.wez.wezterm",
        "co.zeit.hyper",
    ]) {
        return AppCategory::Terminal;
    }
    if exact(&[
        "com.apple.mail",
        "com.microsoft.Outlook",
        "com.readdle.smartemail-Mac",
        "com.superhuman.electron",
        "com.mimestream.Mimestream",
        "it.bloop.airmail2",
        "org.mozilla.thunderbird",
        "com.freron.MailMate",
    ]) {
        return AppCategory::Email;
    }
    if exact(&[
        "com.tinyspeck.slackmacgap",
        "com.apple.MobileSMS",
        "com.hnc.Discord",
        "com.microsoft.teams2",
        "com.microsoft.teams",
        "ru.keepcoder.Telegram",
        "org.telegram.desktop",
        "net.whatsapp.WhatsApp",
        "desktop.WhatsApp",
        "org.whispersystems.signal-desktop",
        "com.facebook.archon",
        "us.zoom.xos",
    ]) {
        return AppCategory::Chat;
    }
    if exact(&[
        "com.apple.iWork.Pages",
        "com.apple.Pages",
        "com.microsoft.Word",
        "com.apple.TextEdit",
        "com.apple.iWork.Keynote",
        "com.apple.iWork.Numbers",
        "com.microsoft.Powerpoint",
        "com.microsoft.Excel",
        "com.literatureandlatte.scrivener3",
        "com.ulyssesapp.mac",
        "abnerworks.Typora",
        "com.google.drivefs",
    ]) {
        return AppCategory::Document;
    }
    if exact(&[
        "com.apple.Notes",
        "md.obsidian",
        "notion.id",
        "net.shinyfrog.bear",
        "com.agiletortoise.Drafts-OSX",
        "com.logseq.logseq",
        "com.lukilabs.lukiapp",
        "com.evernote.Evernote",
    ]) {
        return AppCategory::Notes;
    }
    if is_browser(id) {
        return AppCategory::Browser;
    }
    AppCategory::Other
}

/// Browsers whose focused web area may expose a page URL through Accessibility.
pub fn is_browser(bundle_id: &str) -> bool {
    const BROWSERS: &[&str] = &[
        "com.apple.Safari",
        "com.apple.SafariTechnologyPreview",
        "com.google.Chrome",
        "com.google.Chrome.beta",
        "com.google.Chrome.dev",
        "com.google.Chrome.canary",
        "org.chromium.Chromium",
        "company.thebrowser.Browser", // Arc
        "company.thebrowser.dia",
        "com.microsoft.edgemac",
        "com.microsoft.edgemac.Beta",
        "com.brave.Browser",
        "com.brave.Browser.beta",
        "org.mozilla.firefox",
        "org.mozilla.firefoxdeveloperedition",
        "com.operasoftware.Opera",
        "com.vivaldi.Vivaldi",
        "app.zen-browser.zen",
        "com.kagi.kagimacOS",
    ];
    let id = bundle_id.trim();
    BROWSERS.iter().any(|b| b.eq_ignore_ascii_case(id))
}

/// Human label for a category; used for display only.
pub fn category_label(category: AppCategory) -> &'static str {
    match category {
        AppCategory::Code => "Code",
        AppCategory::Terminal => "Terminal",
        AppCategory::Email => "Email",
        AppCategory::Chat => "Chat",
        AppCategory::Document => "Documents",
        AppCategory::Notes => "Notes",
        AppCategory::Browser => "Web",
        AppCategory::Other => "Other",
    }
}

/// What the platform reader returned for the focused window. Transient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSignal<'a> {
    None,
    /// Absolute file path; `directory` is lexical (a trailing-slash file URL).
    File {
        path: &'a str,
        directory: bool,
    },
    /// Host only; never a path, query or fragment.
    WebHost(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Ancestor {
    key: String,
    /// Folder name, used only if this ancestor's workspace turns visible.
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    None,
    Folder {
        leaf: String,
        /// Eligible ancestors, shallowest first; the last one is the anchor.
        chain: Vec<Ancestor>,
    },
    Web {
        key: String,
        host: String,
    },
}

/// One dictation's transient context: salted keys plus the folder names or host
/// needed to label a workspace at the moment it becomes visible. Never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceObservation {
    pub application_id: String,
    pub category: AppCategory,
    target: Target,
}

fn salted(salt: &str, domain: &str, value: &str, hex: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(salt.as_bytes());
    hash.update([0]);
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update(value.as_bytes());
    let digest = hash.finalize();
    let mut out = String::with_capacity(hex);
    for byte in digest.iter().take(hex / 2) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Folders that are structure inside a project rather than a project root.
/// Descending stops at the first one, so `repo/src/x.rs` anchors at `repo`.
fn inner_folder(name: &str) -> bool {
    const INNER: &[&str] = &[
        "src",
        "source",
        "sources",
        "lib",
        "libs",
        "test",
        "tests",
        "spec",
        "specs",
        "__tests__",
        "doc",
        "docs",
        "app",
        "apps",
        "crates",
        "packages",
        "pkg",
        "cmd",
        "internal",
        "include",
        "scripts",
        "script",
        "bin",
        "build",
        "dist",
        "out",
        "target",
        "node_modules",
        "vendor",
        "third_party",
        "assets",
        "public",
        "static",
        "components",
        "resources",
        "config",
        "examples",
        "benches",
        "fixtures",
        "migrations",
        "tools",
        "testdata",
        "test_data",
    ];
    name.starts_with('.') || INNER.iter().any(|inner| name.eq_ignore_ascii_case(inner))
}

/// Number of leading components that can never be a workspace root (home,
/// Desktop, Documents, Downloads, iCloud Drive root and system/temporary
/// locations), or `None` when nothing below the path may be used at all.
fn floor(parts: &[&str], home: Option<&[&str]>) -> Option<usize> {
    let lower: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
    let at = |i: usize| lower.get(i).map(String::as_str);
    match at(0) {
        None => return None,
        Some("system" | "applications" | "library" | "usr" | "bin" | "sbin" | "etc" | "dev")
        | Some("cores" | "opt") => return None,
        Some("var") => return None,
        Some("private") => {
            return match at(1) {
                Some("tmp") => Some(2),
                _ => None,
            }
        }
        Some("tmp") => return Some(1),
        Some("volumes") => return Some(2),
        _ => {}
    }
    let home_len = match home {
        Some(home)
            if home.len() >= 2
                && parts.len() >= home.len()
                && home
                    .iter()
                    .zip(parts)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b)) =>
        {
            home.len()
        }
        _ if at(0) == Some("users") => 2,
        _ => return Some(1),
    };
    let mut base = home_len;
    match at(base) {
        Some("desktop" | "documents" | "downloads") => base += 1,
        Some("library") => {
            if at(base + 1) != Some("mobile documents") {
                return None;
            }
            // iCloud Drive root or an app's iCloud container (and its Documents).
            base += 3;
            if matches!(at(base), Some("desktop" | "documents" | "downloads")) {
                base += 1;
            }
        }
        _ => {}
    }
    Some(base)
}

fn components(path: &str) -> Option<Vec<&str>> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
        return None;
    }
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.iter().any(|p| *p == "." || *p == "..") {
        return None;
    }
    Some(parts)
}

/// Lowercase host without `www.`, or `None` for anything that is not a plain
/// DNS name or IP literal.
pub fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':' || c == '[' || c == ']'
        });
    valid.then_some(host)
}

impl WorkspaceObservation {
    /// Hash the transient document signal. `home` is the user's home folder,
    /// used only to exclude it and its generic children as roots.
    pub fn new(
        salt: &str,
        application_id: &str,
        signal: DocumentSignal<'_>,
        home: Option<&str>,
    ) -> Self {
        let category = app_category(application_id);
        let target = match signal {
            DocumentSignal::None => Target::None,
            DocumentSignal::WebHost(host) => match normalize_host(host) {
                Some(host) => Target::Web {
                    key: salted(salt, "host", &host, KEY_HEX),
                    host,
                },
                None => Target::None,
            },
            DocumentSignal::File { path, directory } => {
                Self::folder(salt, path, directory, home).unwrap_or(Target::None)
            }
        };
        Self {
            application_id: application_id.trim().into(),
            category,
            target,
        }
    }

    fn folder(salt: &str, path: &str, directory: bool, home: Option<&str>) -> Option<Target> {
        let parts = components(path)?;
        let home_parts = home.and_then(components);
        let base = floor(&parts, home_parts.as_deref())?;
        // A file's folders are its parents; a directory document (Terminal's
        // working directory) is itself a candidate.
        let folders = if directory {
            parts.len()
        } else {
            parts.len().saturating_sub(1)
        };
        let normalized = |end: usize| format!("/{}", parts[..end].join("/").to_lowercase());
        let mut chain = Vec::new();
        for end in (base + 1)..=folders {
            let name = parts[end - 1];
            if inner_folder(name) {
                break;
            }
            chain.push(Ancestor {
                key: salted(salt, "folder", &normalized(end), KEY_HEX),
                name: name.into(),
            });
        }
        if chain.is_empty() {
            return None;
        }
        // Keep the deepest levels: the anchor matters most, shallow containers least.
        if chain.len() > MAX_CHAIN {
            chain.drain(..chain.len() - MAX_CHAIN);
        }
        Some(Target::Folder {
            leaf: salted(salt, "document", &normalized(parts.len()), LEAF_HEX),
            chain,
        })
    }

    /// Salted keys only, shallowest first. For tests and evaluation output.
    pub fn ancestor_keys(&self) -> Vec<String> {
        match &self.target {
            Target::Folder { chain, .. } => chain.iter().map(|a| a.key.clone()).collect(),
            Target::Web { key, .. } => vec![key.clone()],
            Target::None => vec![],
        }
    }

    pub fn has_document(&self) -> bool {
        !matches!(self.target, Target::None)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceNode {
    /// Salted hash of an anchor folder.
    pub key: String,
    /// Salted hashes of distinct documents anchored here (bounded).
    pub leaves: Vec<String>,
    /// Local day numbers on which this anchor was seen (bounded, most recent).
    pub days: Vec<u32>,
    pub count: u32,
    pub last_day: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    #[default]
    Folder,
    Web,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workspace {
    pub key: String,
    pub kind: WorkspaceKind,
    /// Empty while latent; the project name once visible.
    pub label: String,
    pub renamed: bool,
    pub merged_into: Option<String>,
    /// Deleted by the user: never resurfaces and resolves to no workspace.
    pub suppressed: bool,
    pub dictations: u32,
    pub days: Vec<u32>,
    pub first_seen_day: u32,
    pub last_seen_day: u32,
    pub category: AppCategory,
    /// User-selected folder for the inventory miner (explicit opt-in only).
    pub mining_root: Option<String>,
}

impl Workspace {
    pub fn visible(&self) -> bool {
        !self.suppressed && self.merged_into.is_none() && !self.label.is_empty()
    }
    fn latent(&self) -> bool {
        !self.suppressed && self.merged_into.is_none() && self.label.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceState {
    pub schema_version: u32,
    pub nodes: Vec<WorkspaceNode>,
    pub workspaces: Vec<Workspace>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            nodes: vec![],
            workspaces: vec![],
        }
    }
}

fn push_bounded<T: PartialEq + Ord + Clone>(list: &mut Vec<T>, value: T, limit: usize) {
    if !list.contains(&value) {
        list.push(value);
        list.sort();
        if list.len() > limit {
            list.remove(0);
        }
    }
}

fn hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn valid_workspace_label(label: &str) -> bool {
    let label = label.trim();
    !label.is_empty()
        && label.len() <= MAX_LABEL_BYTES
        && !label.chars().any(char::is_control)
        && !label.contains("<|")
}

fn clean_label(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > MAX_LABEL_BYTES {
            break;
        }
        out.push(c);
    }
    let out = out.replace("<|", "<");
    if out.trim().is_empty() {
        "Workspace".into()
    } else {
        out.trim().into()
    }
}

/// Outcome of resolving one dictation's context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceResolution {
    pub selection: ProjectSelection,
    pub context: WorkspaceContext,
}

impl WorkspaceState {
    pub fn validate(&self) -> Result<(), String> {
        let err = || Err("Unsupported or oversized workspace store.".to_string());
        if self.schema_version != WORKSPACE_SCHEMA_VERSION
            || self.nodes.len() > MAX_WORKSPACE_NODES
            || self.workspaces.len() > MAX_WORKSPACES
        {
            return err();
        }
        for node in &self.nodes {
            if !hex(&node.key, KEY_HEX)
                || node.leaves.len() > MAX_LEAVES
                || node.days.len() > MAX_DAYS
                || node.leaves.iter().any(|l| !hex(l, LEAF_HEX))
            {
                return err();
            }
        }
        let mut keys = std::collections::HashSet::new();
        for w in &self.workspaces {
            if !hex(&w.key, KEY_HEX)
                || !keys.insert(&w.key)
                || w.days.len() > MAX_DAYS
                || (!w.label.is_empty() && !valid_workspace_label(&w.label))
                || w.merged_into
                    .as_ref()
                    .is_some_and(|k| !hex(k, KEY_HEX) || *k == w.key)
                || w.mining_root.as_ref().is_some_and(|p| {
                    !p.starts_with('/') || p.len() > 4096 || p.chars().any(char::is_control)
                })
            {
                return err();
            }
        }
        Ok(())
    }

    pub fn workspace(&self, key: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.key == key)
    }

    fn workspace_mut(&mut self, key: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.key == key)
    }

    /// Follow merges (bounded) to the workspace that owns a key.
    pub fn canonical(&self, key: &str) -> Option<&Workspace> {
        let mut current = self.workspace(key)?;
        for _ in 0..MAX_WORKSPACES {
            match &current.merged_into {
                Some(next) => current = self.workspace(next)?,
                None => return Some(current),
            }
        }
        None
    }

    pub fn visible(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.iter().filter(|w| w.visible())
    }

    fn label_taken(&self, label: &str, except: &str) -> bool {
        self.visible()
            .any(|w| w.key != except && w.label.eq_ignore_ascii_case(label.trim()))
    }

    fn unique_label(&self, base: &str, key: &str) -> String {
        let base = clean_label(base);
        if !self.label_taken(&base, key) {
            return base;
        }
        for n in 2..=MAX_WORKSPACES + 1 {
            let suffix = format!(" {n}");
            let mut stem = base.clone();
            while stem.len() + suffix.len() > MAX_LABEL_BYTES {
                stem.pop();
            }
            let candidate = format!("{}{suffix}", stem.trim_end());
            if !self.label_taken(&candidate, key) {
                return candidate;
            }
        }
        format!("Workspace {}", &key[..6])
    }

    /// The registered workspace for this observation: the deepest registered
    /// root on the folder chain (any state), or the web host's row.
    fn registered(&self, observation: &WorkspaceObservation) -> Option<(usize, String)> {
        match &observation.target {
            Target::Folder { chain, .. } => chain
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| self.workspace(&a.key).is_some())
                .map(|(i, a)| (i, a.key.clone())),
            Target::Web { key, .. } => self.workspace(key).map(|_| (0, key.clone())),
            Target::None => None,
        }
    }

    /// Record one dictation and return the key of the workspace it counted
    /// toward (possibly still latent). Deterministic: replaying the same
    /// observation against the same state yields the same state.
    pub fn observe(&mut self, observation: &WorkspaceObservation, day: u32) -> Option<String> {
        let (registered_key, label_source) = match &observation.target {
            Target::None => return None,
            Target::Web { key, host } => {
                if self.workspace(key).is_none() {
                    self.register(key, WorkspaceKind::Web, 0, &[], day);
                }
                (self.workspace(key).map(|_| key.clone()), host.clone())
            }
            Target::Folder { leaf, chain } => {
                let anchor = chain.last()?;
                let node = self.touch_node(&anchor.key, leaf, day);
                // Register the anchor once enough independent documents anchor there.
                if self.workspace(&anchor.key).is_none()
                    && node.leaves.len() >= ROOT_DOCUMENTS
                    && node.days.len() >= ROOT_DAYS
                {
                    let (count, days) = (node.count.saturating_sub(1), node.days.clone());
                    self.register(&anchor.key, WorkspaceKind::Folder, count, &days, day);
                    self.nodes.retain(|n| n.key != anchor.key);
                }
                match self.registered(observation) {
                    Some((index, key)) => (Some(key), chain[index].name.clone()),
                    None => (None, String::new()),
                }
            }
        };
        let key = registered_key?;
        let owner = self.canonical(&key)?.key.clone();
        let category = observation.category;
        let became_visible = {
            let w = self.workspace_mut(&owner)?;
            if w.suppressed {
                return None;
            }
            w.dictations = w.dictations.saturating_add(1);
            push_bounded(&mut w.days, day, MAX_DAYS);
            w.last_seen_day = w.last_seen_day.max(day);
            if category != AppCategory::Other {
                w.category = category;
            }
            w.latent() && w.dictations >= VISIBLE_DICTATIONS && w.days.len() >= VISIBLE_DAYS
        };
        if became_visible {
            // Only the owner's own name labels it; a merged child never relabels.
            let label = if owner == key {
                self.unique_label(&label_source, &owner)
            } else {
                self.unique_label("Workspace", &owner)
            };
            if let Some(w) = self.workspace_mut(&owner) {
                w.label = label;
            }
        }
        Some(owner)
    }

    fn touch_node(&mut self, key: &str, leaf: &str, day: u32) -> WorkspaceNode {
        let index = match self.nodes.iter().position(|n| n.key == key) {
            Some(index) => index,
            None => {
                if self.nodes.len() >= MAX_WORKSPACE_NODES {
                    // Evict the least recently seen anchor (ties: smallest key).
                    if let Some(oldest) = self
                        .nodes
                        .iter()
                        .enumerate()
                        .min_by(|a, b| a.1.last_day.cmp(&b.1.last_day).then(a.1.key.cmp(&b.1.key)))
                        .map(|(i, _)| i)
                    {
                        self.nodes.remove(oldest);
                    }
                }
                self.nodes.push(WorkspaceNode {
                    key: key.into(),
                    ..Default::default()
                });
                self.nodes.len() - 1
            }
        };
        let node = &mut self.nodes[index];
        node.count = node.count.saturating_add(1);
        if !node.leaves.iter().any(|l| l == leaf) && node.leaves.len() < MAX_LEAVES {
            node.leaves.push(leaf.into());
        }
        push_bounded(&mut node.days, day, MAX_DAYS);
        node.last_day = node.last_day.max(day);
        node.clone()
    }

    fn register(&mut self, key: &str, kind: WorkspaceKind, count: u32, days: &[u32], day: u32) {
        if self.workspaces.len() >= MAX_WORKSPACES {
            // Only latent workspaces are evicted; visible and user-managed rows stay.
            let Some(oldest) = self
                .workspaces
                .iter()
                .enumerate()
                .filter(|(_, w)| w.latent() && w.mining_root.is_none())
                .min_by(|a, b| {
                    a.1.last_seen_day
                        .cmp(&b.1.last_seen_day)
                        .then(a.1.key.cmp(&b.1.key))
                })
                .map(|(i, _)| i)
            else {
                return;
            };
            self.workspaces.remove(oldest);
        }
        let mut seen = days.to_vec();
        seen.sort_unstable();
        seen.dedup();
        self.workspaces.push(Workspace {
            key: key.into(),
            kind,
            dictations: count,
            first_seen_day: seen.first().copied().unwrap_or(day),
            last_seen_day: day,
            days: seen,
            ..Default::default()
        });
    }

    /// The visible workspace that owns this observation, if any. Latent and
    /// suppressed roots never capture a dictation; a latent subfolder root
    /// defers to a visible ancestor root.
    pub fn visible_for(&self, observation: &WorkspaceObservation) -> Option<&Workspace> {
        match &observation.target {
            Target::None => None,
            Target::Web { key, .. } => self.canonical(key).filter(|w| w.visible()),
            Target::Folder { chain, .. } => {
                for ancestor in chain.iter().rev() {
                    let Some(w) = self.workspace(&ancestor.key) else {
                        continue;
                    };
                    if w.suppressed {
                        return None;
                    }
                    if let Some(owner) = self.canonical(&w.key) {
                        if owner.suppressed {
                            return None;
                        }
                        if owner.visible() {
                            return Some(owner);
                        }
                    }
                }
                None
            }
        }
    }

    /// Rename a visible workspace. Returns the previous label.
    pub fn rename(&mut self, key: &str, label: &str) -> Result<String, String> {
        let label = label.trim();
        if !valid_workspace_label(label) {
            return Err("Workspace names must be plain text up to 64 bytes.".into());
        }
        if !self.workspace(key).is_some_and(Workspace::visible) {
            return Err("That workspace is not available.".into());
        }
        if self.label_taken(label, key) {
            return Err("Another workspace already has that name. Merge them instead.".into());
        }
        let w = self.workspace_mut(key).expect("checked above");
        let old = std::mem::replace(&mut w.label, label.into());
        w.renamed = true;
        Ok(old)
    }

    /// Merge `from` into `into`; both visible. Returns (from label, into label).
    pub fn merge(&mut self, from: &str, into: &str) -> Result<(String, String), String> {
        let into = self
            .canonical(into)
            .filter(|w| w.visible())
            .map(|w| w.key.clone())
            .ok_or("Choose a workspace to merge into.")?;
        if from == into || !self.workspace(from).is_some_and(Workspace::visible) {
            return Err("Choose two different workspaces.".into());
        }
        let (old, dictations, days) = {
            let w = self.workspace_mut(from).expect("checked above");
            w.merged_into = Some(into.clone());
            w.mining_root = None;
            (
                std::mem::take(&mut w.label),
                w.dictations,
                std::mem::take(&mut w.days),
            )
        };
        let target = self.workspace_mut(&into).expect("checked above");
        target.dictations = target.dictations.saturating_add(dictations);
        for day in days {
            push_bounded(&mut target.days, day, MAX_DAYS);
        }
        Ok((old, target.label.clone()))
    }

    /// Delete (suppress) a workspace: it never resurfaces and its folder or
    /// host resolves to no workspace. Returns the removed label.
    pub fn delete(&mut self, key: &str) -> Result<String, String> {
        let w = self
            .workspace_mut(key)
            .filter(|w| !w.suppressed)
            .ok_or("That workspace is not available.")?;
        w.suppressed = true;
        w.mining_root = None;
        w.merged_into = None;
        Ok(std::mem::take(&mut w.label))
    }

    pub fn set_mining_root(&mut self, key: &str, root: Option<String>) -> Result<(), String> {
        if root.as_ref().is_some_and(|p| {
            !p.starts_with('/') || p.len() > 4096 || p.chars().any(char::is_control)
        }) {
            return Err("Choose an absolute folder.".into());
        }
        let w = self
            .workspace_mut(key)
            .filter(|w| w.visible() && w.kind == WorkspaceKind::Folder)
            .ok_or("Only a visible folder workspace can learn words from a folder.")?;
        w.mining_root = root;
        Ok(())
    }
}

/// Final precedence: manual project → existing rule → visible workspace →
/// app category → shared. `rules` is the existing rule resolver's answer.
/// Categories never partition vocabulary: they resolve to shared scope and are
/// carried in the context as a ranking signal.
pub fn resolve_workspace(
    rules: ProjectSelection,
    automatic: bool,
    state: &WorkspaceState,
    observation: Option<&WorkspaceObservation>,
) -> WorkspaceResolution {
    let mut context = WorkspaceContext::default();
    if let Some(observation) = observation {
        context.application_id = observation.application_id.clone();
        context.category = observation.category;
    }
    if !rules.project.is_empty() || !automatic {
        return WorkspaceResolution {
            selection: rules,
            context,
        };
    }
    if let Some(workspace) = observation.and_then(|o| state.visible_for(o)) {
        context.workspace_key = workspace.key.clone();
        context.workspace_label = workspace.label.clone();
        return WorkspaceResolution {
            selection: ProjectSelection {
                project: workspace.label.clone(),
                source: ProjectSource::Workspace,
                rule_id: None,
            },
            context,
        };
    }
    let selection = if context.category != AppCategory::Other
        && matches!(rules.source, ProjectSource::Shared)
    {
        ProjectSelection {
            project: String::new(),
            source: ProjectSource::Category,
            rule_id: None,
        }
    } else {
        rules
    };
    WorkspaceResolution { selection, context }
}

#[cfg(test)]
mod tests;
