//! The downloader tree the manifest becomes: `PatcherDownloader` and its
//! three kinds (SETUP `patcherdownloader.cpp`, `distributiondownloader.cpp`,
//! `directorydownloader.cpp`, `filedownloader.cpp`), and
//! `PatcherDistributionInfo` (`patcherdistributioninfo_impl.cpp`): one
//! distribution's attributes. Nodes live in an arena (`Tree::nodes`), linked
//! as the game links them (parent, first child, previous and next sibling).
//!
//! What SETUP never reaches is left out where it would be: delta downloads
//! (`NeedsPartialPatch` is 0, so a matching `<DeltaFile>` only lends the bar
//! its sizes), a Restricted distribution's file (an `fopen` of a bare name,
//! never found on a PS2), test and beta flags (SETUP passes 0), time stamps
//! (`TimeDiscrepancy` is 0 on a PS2).

use super::{Api, image::crc32};
use crate::pregame::station::xml::{Element, atoi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Distribution,
    Directory,
    File,
}

/// One `<DeltaFile>` of a `<File>`: only its sizes and CRC are kept.
#[derive(Clone, Debug, Default)]
pub struct Delta {
    pub download_size: i64,
    pub total_size: i64,
    pub crc: u32,
}

/// A node of the tree.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub name: Option<String>,
    pub server: Option<String>,
    pub port: i32,
    pub remote_path: Option<String>,
    pub local_path: Option<String>,
    /// a distribution's "Root"
    pub default_root: Option<String>,
    /// 0 no, 1 yes, 2 as the parent does
    pub use_crc_mode: i32,
    pub ignore_time_stamp: bool,
    pub install_only: bool,
    /// TestFile, BetaFile
    pub special: [bool; 2],
    pub xtra: bool,
    pub scan_as_test: bool,
    pub need_download: i32,
    pub delete: bool,
    pub parent: Option<usize>,
    pub prev: Option<usize>,
    pub next: Option<usize>,
    pub child: Option<usize>,
    /// the distribution info this node belongs to (`m_info`)
    pub info: Option<usize>,
    // a directory's and a file's
    pub local_name: Option<String>,
    // a file's
    pub total_size: i64,
    pub download_size: i64,
    pub crc: u32,
    pub compressed: bool,
    pub restart: bool,
    pub temp_name: Option<String>,
    pub deltas: Vec<Delta>,
    pub delta: Option<usize>,
    pub discrepancy: bool,
    pub local_crc: u32,
    // a distribution's: ScanFiles, GetFileCount, GetDownloadSize, GetTotalSize, checked
    pub scan: [bool; 5],
}

impl Node {
    fn new(kind: Kind) -> Node {
        Node {
            kind,
            name: None,
            server: None,
            port: 0,
            remote_path: None,
            local_path: None,
            default_root: None,
            // the constructor: UseCRC inherits, time stamps ignored
            use_crc_mode: 2,
            ignore_time_stamp: true,
            install_only: false,
            special: [false; 2],
            xtra: false,
            scan_as_test: false,
            need_download: 0,
            delete: false,
            parent: None,
            prev: None,
            next: None,
            child: None,
            info: None,
            local_name: None,
            total_size: 0,
            download_size: 0,
            crc: 0,
            compressed: true,
            restart: false,
            temp_name: None,
            deltas: Vec::new(),
            delta: None,
            discrepancy: false,
            local_crc: 0,
            scan: [false; 5],
        }
    }
}

/// `PatcherDistributionInfo_Impl`.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub name: Option<String>,
    pub title: Option<String>,
    pub path: Option<String>,
    pub server: Option<String>,
    pub port: i32,
    pub optional: bool,
    pub restricted: bool,
    pub restricted_file: Option<String>,
    pub restricted_file_exists: bool,
    /// what a download takes: not Optional
    pub selected: bool,
    /// the distribution's release notes, by Version
    pub changes: Vec<(i16, String)>,
    pub downloader: usize,
}

/// The tree.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub infos: Vec<Info>,
}

// ---- PullXMLAttribute* (pullxml.cpp) ----

pub fn attr(e: &Element, name: &str) -> Option<String> {
    e.prop(name).map(str::to_string)
}

/// `PullXMLAttributeLong`: newlib's `atoi`, 0 when absent.
pub fn attr_long(e: &Element, name: &str) -> i32 {
    e.prop(name).map_or(0, atoi)
}

/// `PullXMLAttributeBool`: y, yes, t, true, 1 (any case).
pub fn attr_bool(e: &Element, name: &str) -> bool {
    e.prop(name).is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "y" | "yes" | "t" | "true" | "1"))
}

/// `PullDefinedXMLAttBool`: 1 yes, 0 no, 2 neither (absent, or another word).
pub fn attr_defined(e: &Element, name: &str) -> i32 {
    match e.prop(name).map(str::to_ascii_lowercase).as_deref() {
        Some("y" | "yes" | "t" | "true" | "on" | "1") => 1,
        Some("n" | "no" | "f" | "false" | "off" | "0") => 0,
        _ => 2,
    }
}

/// `PullXMLAttributeBoolDef`.
pub fn attr_bool_def(e: &Element, name: &str, def: bool) -> bool {
    match attr_defined(e, name) {
        1 => true,
        0 => false,
        _ => def,
    }
}

/// `DirectoryDownloader::GetFullPath`: a LocalPath stands for the whole path.
fn dir_full_path(name: Option<&str>, path: Option<&str>, root: Option<&str>) -> Option<String> {
    if let Some(p) = path {
        return match p.to_ascii_lowercase().as_str() {
            "::currentdirectory::" => root.map(str::to_string),
            // getenv is NULL on the PS2
            "::homedirectory::" | "::windowshome::" => None,
            _ => Some(p.to_string()),
        };
    }
    match (root, name) {
        (Some(r), Some(n)) => Some(format!("{r}/{n}")),
        (Some(r), None) => Some(r.to_string()),
        (None, Some(n)) => Some(n.to_string()),
        (None, None) => None,
    }
}

/// `FileDownloader::GetFullPath`: LocalPath (or RemotePath) + "/" + name,
/// else root + "/" + name, else the name. An empty RemotePath is a path:
/// `<File RemotePath="" Name="x.gz">` fetches "/x.gz".
fn file_full_path(name: Option<&str>, path: Option<&str>, root: Option<&str>) -> Option<String> {
    let (mut have_path, mut have_root) = (path.is_some(), root.is_some());
    if let Some(p) = path
        && matches!(p.to_ascii_lowercase().as_str(), "::homedirectory::" | "::desktop::" | "::startmenu::" | "::windowshome::")
    {
        have_path = false;
        have_root = false;
    }
    match (have_path, have_root, name) {
        (true, _, Some(n)) => Some(format!("{}/{n}", path.unwrap())),
        (true, _, None) => path.map(str::to_string),
        (false, true, Some(n)) => Some(format!("{}/{n}", root.unwrap())),
        (false, true, None) => root.map(str::to_string),
        (false, false, n) => n.map(str::to_string),
    }
}

/// The command line a file is checked against (`PatchPatcher`): the PS2's.
const COMMAND_LINE: &str = "host0:/main.elf";

impl Tree {
    fn add(&mut self, kind: Kind) -> usize {
        self.nodes.push(Node::new(kind));
        self.nodes.len() - 1
    }

    /// `NeedDownload(flags)`: SETUP passes 0 (0x10 counts everything).
    pub fn need_download_for(&self, n: usize, flags: i32) -> i32 {
        let d = &self.nodes[n];
        if flags & 0x10 == 0 && ((flags & 4 != 0 && d.special[0]) || (flags & 8 != 0 && d.special[1])) {
            return 0;
        }
        d.need_download
    }

    /// `GetServer`: this node's Server, else its parent's.
    pub fn server(&self, mut n: usize) -> Option<String> {
        loop {
            let d = &self.nodes[n];
            match (d.server.as_ref(), d.parent) {
                (Some(s), _) => return Some(s.clone()),
                (None, Some(p)) => n = p,
                (None, None) => return None,
            }
        }
    }

    /// `GetPort`: this node's Port, else its parent's.
    pub fn port(&self, mut n: usize) -> i32 {
        loop {
            let d = &self.nodes[n];
            match (d.port, d.parent) {
                (0, Some(p)) => n = p,
                (p, _) => return p,
            }
        }
    }

    /// `UsingCRC`: a file that restarts the patcher always; else the node's
    /// setting, or its parent's.
    pub fn using_crc(&self, n: usize, quick_scan: bool) -> bool {
        let d = &self.nodes[n];
        if d.kind == Kind::File && d.restart {
            return true;
        }
        if quick_scan {
            return false;
        }
        match d.use_crc_mode {
            1 => true,
            2 => d.parent.is_some_and(|p| self.using_crc(p, quick_scan)),
            _ => false,
        }
    }

    fn scan_as_test(&self, n: usize) -> bool {
        let d = &self.nodes[n];
        (d.kind == Kind::File && (d.special[0] || d.special[1] || d.xtra)) || d.scan_as_test
    }

    fn children(&self, n: usize) -> Vec<usize> {
        let mut v = Vec::new();
        let mut c = self.nodes[n].child;
        while let Some(i) = c {
            v.push(i);
            c = self.nodes[i].next;
        }
        v
    }

    /// Links `child` after `prev` under `parent`.
    fn link(&mut self, parent: usize, child: usize, prev: Option<usize>) {
        if self.nodes[parent].child.is_none() {
            self.nodes[parent].child = Some(child);
        }
        if let Some(p) = prev {
            self.nodes[p].next = Some(child);
        }
    }

    /// `PatcherDownloader::Initialize` (the attributes every node has).
    fn initialize_base(&mut self, n: usize, e: &Element, parent: Option<usize>, prev: Option<usize>) {
        let (p_delete, p_ignore, p_install) = parent.map_or((false, false, false), |p| (self.nodes[p].delete, self.nodes[p].ignore_time_stamp, self.nodes[p].install_only));
        let d = &mut self.nodes[n];
        d.parent = parent;
        d.prev = prev;
        d.name = attr(e, "Name");
        d.local_path = attr(e, "LocalPath");
        d.remote_path = attr(e, "RemotePath");
        d.server = attr(e, "Server");
        d.port = attr_long(e, "Port");
        d.delete = attr_bool(e, "Delete");
        d.ignore_time_stamp = attr_bool_def(e, "IgnoreTimeStamp", false);
        d.install_only = attr_bool_def(e, "InstallOnly", false);
        d.special = [attr_bool_def(e, "TestFile", false), attr_bool_def(e, "BetaFile", false)];
        d.xtra = attr_bool_def(e, "XtraFile", false);
        d.use_crc_mode = if d.special[0] || d.special[1] || d.xtra { 1 } else { attr_defined(e, "UseCRC") };
        if parent.is_some() {
            d.delete |= p_delete;
            d.ignore_time_stamp |= p_ignore;
            d.install_only |= p_install;
        }
        if d.delete {
            d.need_download = 0;
        }
    }

    /// `CalculateHierarchicalProperties`: a node needs downloading when any
    /// child does (it never clears the flag).
    pub fn calculate(&mut self, n: usize) {
        for c in self.children(n) {
            self.calculate(c);
            if self.need_download_for(c, 0x10) != 0 {
                self.nodes[n].need_download = 1;
            }
            if self.scan_as_test(c) {
                self.nodes[n].scan_as_test = true;
            }
        }
        if self.nodes[n].delete && self.nodes[n].kind == Kind::Distribution {
            self.nodes[n].need_download = 0;
        }
    }

    pub fn reset_scan(&mut self, n: usize, flags: usize) {
        for c in self.children(n) {
            self.reset_scan(c, flags);
        }
        if self.nodes[n].kind == Kind::Distribution && flags < 5 {
            self.nodes[n].scan[flags] = false;
        }
    }

    // ---- sizes ----

    /// `GetTotalSize`; `download` picks `GetDownloadSize`.
    pub fn size(&mut self, n: usize, flags: i32, download: bool) -> i64 {
        match self.nodes[n].kind {
            Kind::File => {
                if self.need_download_for(n, flags) == 0 {
                    return 0;
                }
                let d = &self.nodes[n];
                match (d.delta, download) {
                    (Some(i), true) => d.deltas[i].download_size,
                    (Some(i), false) => d.deltas[i].total_size,
                    (None, true) => d.download_size,
                    (None, false) => d.total_size,
                }
            }
            Kind::Directory => self.children(n).into_iter().map(|c| self.size(c, flags, download)).sum(),
            Kind::Distribution => {
                let flag = if download { 2 } else { 3 };
                if self.nodes[n].scan[flag] {
                    return 0;
                }
                self.nodes[n].scan[flag] = true;
                let selected = self.nodes[n].info.is_some_and(|i| self.infos[i].selected);
                let mut total = 0;
                let mut c = self.nodes[n].child;
                // the first plain child starts a walk over it and every
                // sibling after it, distributions included
                while let Some(i) = c {
                    if self.nodes[i].kind != Kind::Distribution {
                        if selected {
                            let mut e = Some(i);
                            while let Some(j) = e {
                                total += self.size(j, flags, download);
                                e = self.nodes[j].next;
                            }
                            c = None;
                        } else {
                            c = self.nodes[i].next;
                        }
                    } else {
                        total += self.size(i, flags, download);
                        c = self.nodes[i].next;
                    }
                }
                total
            }
        }
    }

    // ---- building ----

    /// `PatcherDistributionInfo_Impl::Initialize`: the top level's info and
    /// its downloader, over the `<Product>`.
    pub fn initialize_product(api: &mut Api, e: &Element) -> usize {
        let info = api.tree.infos.len();
        api.tree.infos.push(Info::default());
        let d = api.tree.add(Kind::Distribution);
        api.tree.infos[info].downloader = d;
        api.tree.nodes[d].info = Some(info);
        api.tree.set_info_xml(info, e);
        Self::initialize_distribution(api, d, e, None, None);
        info
    }

    /// `PatcherDistributionInfo_Impl::SetXMLInfo`.
    fn set_info_xml(&mut self, info: usize, e: &Element) {
        let parent_info = {
            let d = self.infos[info].downloader;
            self.nodes.get(d).and_then(|n| n.parent).filter(|&p| self.nodes[p].kind == Kind::Distribution).and_then(|p| self.nodes[p].info)
        };
        let i = &mut self.infos[info];
        if i.name.is_none() {
            i.name = attr(e, "Name");
        }
        if i.title.is_none() {
            i.title = attr(e, "Title");
        }
        if i.path.is_none() {
            i.path = attr(e, "RemotePath");
        }
        if i.server.is_none() {
            i.server = attr(e, "Server");
        }
        if i.port == 0 {
            i.port = attr_long(e, "Port");
        }
        i.optional = attr_bool(e, "Optional");
        i.restricted = attr_bool(e, "Restricted");
        if i.restricted_file.is_none() {
            i.restricted_file = attr(e, "RestrictedFile");
        }
        i.restricted_file_exists = true;
        i.selected = !i.optional;
        if i.restricted {
            // fopen of a bare name: never there on a PS2
            i.restricted_file_exists = false;
            i.selected = false;
        }
        if let Some(p) = parent_info {
            let (pr, pe, po) = (self.infos[p].restricted, self.infos[p].restricted_file_exists, self.infos[p].optional);
            let i = &mut self.infos[info];
            if !i.restricted {
                i.restricted = pr;
                i.restricted_file_exists = pe;
            }
            if !i.optional {
                i.optional = po;
            }
        }
    }

    /// `DistributionDownloader::Initialize` then `TestAndFinalizeDistribution`.
    fn initialize_distribution(api: &mut Api, d: usize, e: &Element, parent: Option<usize>, prev: Option<usize>) -> bool {
        api.tree.initialize_base(d, e, parent, prev);
        if api.tree.nodes[d].info.is_none() {
            let info = api.tree.infos.len();
            api.tree.infos.push(Info { downloader: d, ..Default::default() });
            api.tree.nodes[d].info = Some(info);
        }
        Self::test_and_finalize(api, d, e)
    }

    fn test_and_finalize(api: &mut Api, d: usize, e: &Element) -> bool {
        let info = api.tree.nodes[d].info.unwrap();
        api.tree.set_info_xml(info, e);
        api.tree.nodes[d].default_root = attr(e, "Root");
        if e.children.is_empty() && api.tree.nodes[d].remote_path.is_some() {
            return Self::external_tree(api, d);
        }
        let i = &api.tree.infos[info];
        if !i.restricted || i.restricted_file_exists {
            Self::distribution_children(api, d, e);
            api.tree.nodes[d].scan[4] = true;
        }
        true
    }

    /// `GetExternalTree`: a distribution with no children and a RemotePath
    /// is an XML file of its own. (The PS2's fallback through a temporary
    /// card image can never parse, and is not ported.)
    fn external_tree(api: &mut Api, d: usize) -> bool {
        let (server, port, path) = (api.tree.server(d), api.tree.port(d), api.tree.nodes[d].remote_path.clone());
        let Some(text) = api.download_memory_file(0x19000, server.as_deref(), port, path.as_deref(), true) else {
            api.cb.on_error(44, &format!("Error accessing external xml file: {}:{port}/{}.\n", server.as_deref().unwrap_or("(null)"), path.as_deref().unwrap_or("(null)")));
            return false;
        };
        let Some(root) = crate::pregame::station::xml::parse(&text) else {
            api.cb.on_error(40, &format!("Error parsing xml file: {}.\n", path.as_deref().unwrap_or("(null)")));
            return false;
        };
        if !root.name.eq_ignore_ascii_case("VerantPatcher") {
            api.cb.on_error(41, &format!("Document {} of the wrong type, VerantPatcher expected.\n", path.as_deref().unwrap_or("(null)")));
            return false;
        }
        let mut done = true;
        for n in &root.children {
            // Initialize again for each node: each replaces the last's attributes
            let (p_delete, p_crc) = api.tree.nodes[d].parent.map_or((false, false), |p| (api.tree.nodes[p].delete, api.tree.using_crc(p, api.quick_scan)));
            let node = &mut api.tree.nodes[d];
            node.name = attr(n, "Name");
            node.local_path = attr(n, "LocalPath");
            node.remote_path = attr(n, "RemotePath");
            node.server = attr(n, "Server");
            node.port = attr_long(n, "Port");
            node.use_crc_mode = attr_bool(n, "UseCRC") as i32;
            node.delete = attr_bool(n, "Delete") || p_delete;
            if p_crc {
                node.use_crc_mode = 1;
            }
            if !Self::test_and_finalize(api, d, n) {
                done = false;
            }
        }
        done
    }

    /// `DistributionDownloader::InitializeChildren`: a child distribution
    /// makes its own info, the rest share this one's; a `<Changes>` is the
    /// distribution's release notes (fetched here too, so the product's are
    /// fetched twice, as on a PS2).
    fn distribution_children(api: &mut Api, d: usize, e: &Element) {
        let info = api.tree.nodes[d].info;
        let mut prev = None;
        for n in &e.children {
            let kind = match n.name.to_ascii_lowercase().as_str() {
                "distribution" => Kind::Distribution,
                "directory" => Kind::Directory,
                "file" => Kind::File,
                "changes" => {
                    Self::distribution_changes(api, d, n);
                    continue;
                }
                _ => continue,
            };
            let c = api.tree.add(kind);
            match kind {
                Kind::Distribution => {
                    Self::initialize_distribution(api, c, n, Some(d), prev);
                }
                _ => {
                    api.tree.nodes[c].info = info;
                    Self::initialize_child(api, c, n, Some(d), prev, None, None);
                }
            }
            api.tree.link(d, c, prev);
            prev = Some(c);
        }
    }

    /// A distribution's `SetChangesFile`.
    fn distribution_changes(api: &mut Api, d: usize, e: &Element) {
        let server = attr(e, "Server").or_else(|| api.tree.server(d));
        let port = match attr_long(e, "Port") {
            0 => api.tree.port(d),
            p => p,
        };
        let (Some(remote), Some(server)) = (attr(e, "RemotePath"), server) else { return };
        if let Some(text) = api.download_memory_file(0x1F400, Some(&server), port, Some(&remote), attr_bool(e, "Compressed")) {
            let version = attr_long(e, "Version") as i16;
            let info = api.tree.nodes[d].info.unwrap();
            insert_changes(&mut api.tree.infos[info].changes, version, String::from_utf8_lossy(&text).into_owned());
        }
    }

    /// A directory's or a file's `Initialize`.
    fn initialize_child(api: &mut Api, n: usize, e: &Element, parent: Option<usize>, prev: Option<usize>, root: Option<&str>, path: Option<&str>) {
        api.tree.initialize_base(n, e, parent, prev);
        let local_name = attr(e, "LocalName");
        let (local, remote) = (api.tree.nodes[n].local_path.take(), api.tree.nodes[n].remote_path.take());
        match api.tree.nodes[n].kind {
            Kind::Directory => {
                let name = api.tree.nodes[n].name.clone();
                let d = &mut api.tree.nodes[n];
                d.local_name = local_name;
                if d.local_name.is_some() {
                    d.local_path = dir_full_path(d.local_name.as_deref(), local.as_deref(), root);
                } else {
                    d.local_path = dir_full_path(name.as_deref(), local.as_deref(), root);
                    d.local_name = name.clone();
                }
                d.remote_path = dir_full_path(name.as_deref(), remote.as_deref(), path);
                let (lp, rp, info) = (d.local_path.clone(), d.remote_path.clone(), d.info);
                let mut prev = None;
                for c in &e.children {
                    if api.cancel {
                        return;
                    }
                    let kind = match c.name.to_ascii_lowercase().as_str() {
                        "directory" => Kind::Directory,
                        "file" => Kind::File,
                        _ => continue,
                    };
                    let i = api.tree.add(kind);
                    api.tree.nodes[i].info = info;
                    Self::initialize_child(api, i, c, Some(n), prev, lp.as_deref(), rp.as_deref());
                    api.tree.link(n, i, prev);
                    prev = Some(i);
                }
            }
            Kind::File => {
                // SetXMLInfo
                {
                    let d = &mut api.tree.nodes[n];
                    d.local_name = local_name;
                    d.total_size = attr_long(e, "TotalSize") as i64;
                    d.download_size = attr_long(e, "DownloadSize") as i64;
                    d.restart = attr_bool(e, "Restart");
                    d.temp_name = attr(e, "TempFilename");
                    d.crc = attr_long(e, "CRC") as u32;
                    d.compressed = attr_bool_def(e, "Compressed", true);
                }
                // a file the manifest gives no CRC is not checked by one
                if api.tree.using_crc(n, api.quick_scan) && api.tree.nodes[n].crc == 0 {
                    api.tree.nodes[n].use_crc_mode = 0;
                }
                let name = api.tree.nodes[n].name.clone();
                let d = &mut api.tree.nodes[n];
                if d.local_name.is_some() {
                    d.local_path = file_full_path(d.local_name.as_deref(), local.as_deref(), root);
                } else {
                    d.local_path = file_full_path(name.as_deref(), local.as_deref(), root);
                    d.local_name = name.clone();
                }
                d.remote_path = file_full_path(name.as_deref(), remote.as_deref(), path);
                for c in e.children.iter().filter(|c| c.name.eq_ignore_ascii_case("DeltaFile")) {
                    // AddDeltaFile: kept only when a server is known
                    if attr(c, "Server").is_some() || d.server.is_some() || d.parent.is_some() {
                        d.deltas.push(Delta { download_size: attr_long(c, "DownloadSize") as i64, total_size: attr_long(c, "TotalSize") as i64, crc: attr_long(c, "CRC") as u32 });
                    }
                }
                if d.local_path.is_none() {
                    d.need_download = 0;
                }
                Self::check_need_download(api, n);
            }
            Kind::Distribution => unreachable!(),
        }
    }

    /// `FileDownloader::CheckForNeedDownload`: by CRC when the file uses one,
    /// else by SIZE (a missing image, or one whose size is not TotalSize; a
    /// TotalSize of 0 matches anything).
    fn check_need_download(api: &mut Api, n: usize) -> bool {
        if api.tree.nodes[n].parent.is_some_and(|p| api.tree.nodes[p].delete) {
            return false;
        }
        if let Some(local) = api.tree.nodes[n].local_name.clone() {
            if api.tree.nodes[n].temp_name.is_none() {
                let mut t = local.clone().into_bytes();
                if let Some(c) = t.first_mut() {
                    *c = b'_';
                }
                api.tree.nodes[n].temp_name = Some(String::from_utf8_lossy(&t).into_owned());
            }
            let tmp = api.tree.nodes[n].temp_name.clone().unwrap().to_ascii_lowercase();
            if COMMAND_LINE.contains(&tmp) {
                let d = &mut api.tree.nodes[n];
                d.need_download = 0;
                d.delete = false;
                return true;
            }
        }
        if api.tree.nodes[n].delete {
            return true;
        }
        api.tree.nodes[n].need_download = 0;
        let local = api.tree.nodes[n].local_path.clone().unwrap_or_default();
        let have = api.images.find_ref(&local).map(|im| (im.data.len() as i64, im.crc));
        let discrepancy = if api.tree.using_crc(n, api.quick_scan) {
            // CrcDiscrepancy: no CRC in the manifest counts as different
            let d = &mut api.tree.nodes[n];
            d.discrepancy = true;
            d.local_crc = have.map_or(0, |h| h.1);
            d.crc == 0 || d.crc != d.local_crc
        } else {
            let d = &mut api.tree.nodes[n];
            let size_differs = match have {
                None => true,
                Some(_) if d.total_size == 0 => false,
                Some((len, _)) => len != d.total_size,
            };
            if size_differs {
                d.discrepancy = true;
                if !d.deltas.is_empty() {
                    d.local_crc = have.map_or(0, |h| h.1);
                }
            }
            size_differs
        };
        if discrepancy {
            let d = &mut api.tree.nodes[n];
            d.need_download = 1;
            // PartialPatchCheck: a delta from the copy we have lends its sizes
            if d.discrepancy && d.local_crc != 0 {
                d.delta = d.deltas.iter().position(|x| x.crc == d.local_crc);
            }
        }
        true
    }

    // ---- downloading ----

    /// `Download(flags)`: 0 done, 100 stops everything, another code an error.
    pub fn download(api: &mut Api, n: usize, flags: i32) -> i32 {
        match api.tree.nodes[n].kind {
            Kind::File => Self::download_file(api, n, flags),
            Kind::Directory => {
                if api.tree.need_download_for(n, flags) == 0 {
                    return 0;
                }
                let mut result = 0;
                if !api.tree.nodes[n].delete {
                    let mut c = api.tree.nodes[n].child;
                    while let Some(i) = c {
                        if result == 100 {
                            break;
                        }
                        let r = Self::download(api, i, flags);
                        if r != 0 && result != 100 {
                            result = r;
                        }
                        c = api.tree.nodes[i].next;
                    }
                    if result == 0 {
                        api.tree.nodes[n].need_download = 0;
                    }
                }
                result
            }
            Kind::Distribution => {
                let selected = api.tree.nodes[n].info.is_some_and(|i| api.tree.infos[i].selected);
                let mut result = 0;
                let mut c = api.tree.nodes[n].child;
                while let Some(i) = c {
                    if result == 100 {
                        break;
                    }
                    if api.tree.nodes[i].kind != Kind::Distribution {
                        if selected {
                            // the inner walk starts again at each sibling: a
                            // file that failed but for 100 is tried once more
                            // per file after it, as on a PS2
                            let mut e = Some(i);
                            while let Some(j) = e {
                                if result == 100 {
                                    break;
                                }
                                let r = Self::download(api, j, flags);
                                if r != 0 && result != 100 {
                                    result = r;
                                }
                                e = api.tree.nodes[j].next;
                            }
                            if result == 0 {
                                api.tree.nodes[n].need_download = 0;
                            }
                        }
                    } else {
                        let r = Self::download(api, i, flags);
                        if r != 0 && result != 100 {
                            result = r;
                        }
                    }
                    c = api.tree.nodes[i].next;
                }
                result
            }
        }
    }

    /// `FileDownloader::Download`: always the whole file; then the image
    /// must exist and carry the manifest's CRC, UseCRC or not, or the file
    /// failed (100 for that, 20 for any other code). A Restart file then
    /// finishes the patch and asks for a restart, which cancels the rest.
    fn download_file(api: &mut Api, n: usize, flags: i32) -> i32 {
        if api.tree.nodes[n].delete || api.tree.need_download_for(n, flags) == 0 {
            return 0;
        }
        let d = api.tree.nodes[n].clone();
        let target = d.local_path.clone();
        let mut result = api.download_file(api.tree.server(n).as_deref(), d.remote_path.as_deref(), target.as_deref(), d.compressed, d.download_size, d.total_size);
        match api.images.find_ref(target.as_deref().unwrap_or("")) {
            Some(im) => {
                let crc = crc32(&im.data);
                if crc != d.crc {
                    super::say(&format!("patcher: {}: CRC {crc}, the manifest says {}", target.as_deref().unwrap_or(""), d.crc));
                    result = 100;
                }
            }
            None => result = 100,
        }
        if result != 0 {
            if result != 100 {
                result = 20;
            }
            api.cb.on_error(result, &format!("Download error for {}\n", target.as_deref().unwrap_or("(null)")));
            return result;
        }
        api.tree.nodes[n].need_download = 0;
        if d.restart {
            api.cb.on_finish();
            super::say(&format!("patcher: {} asks for a restart; the patch is cancelled", d.local_name.as_deref().unwrap_or("")));
            api.cancel = true;
        }
        0
    }

    /// `Delete`: every Delete entry marks its image, which the folder's
    /// write removes.
    pub fn delete_old(api: &mut Api, n: usize) {
        for c in api.tree.children(n) {
            Self::delete_old(api, c);
        }
        if !api.tree.nodes[n].delete {
            return;
        }
        if api.tree.nodes[n].kind == Kind::File {
            let local = api.tree.nodes[n].local_path.clone().unwrap_or_default();
            if let Some(im) = api.images.find(&local) {
                im.deleted = true;
            }
        }
        if api.tree.nodes[n].kind != Kind::Distribution {
            api.tree.nodes[n].delete = false;
        }
    }
}

/// `SetChangesFile`'s list, kept in Version order as the PS2 threads it
/// (its walk does not stop at the first later version; the head is what is
/// shown, `GetTextChanges`).
pub fn insert_changes(list: &mut Vec<(i16, String)>, version: i16, text: String) {
    match list.iter().position(|(v, _)| version < *v) {
        Some(i) => list.insert(i, (version, text)),
        None => list.push((version, text)),
    }
}
