//! Lecture minimale et en lecture seule d'un dépôt Git local.
//!
//! On n'a besoin que de la branche courante et des sujets des derniers
//! commits. Plutôt qu'une bibliothèque Git complète (gix embarque un
//! module de protocole et de transport, refusé par le verrou anti-réseau),
//! ce module lit directement le format de stockage de Git :
//!
//! - `HEAD`, références « loose » et `packed-refs` ;
//! - objets « loose » (zlib) et paquets (`.idx` v2 + `.pack`, y compris les
//!   deltas `OFS_DELTA` / `REF_DELTA`).
//!
//! Aucun programme n'est lancé, aucune configuration n'est interprétée,
//! rien n'est écrit. Tout objet manquant (clone superficiel) ou format non
//! géré (dépôt SHA-256) arrête simplement la lecture.

use std::collections::{BinaryHeap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use flate2::read::ZlibDecoder;
use regex::Regex;

use super::GitInfo;

static CONVENTIONAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([^)]+\))?!?: ").unwrap()
});

/// Taille maximale d'un objet décompressé (un commit fait quelques Ko).
const MAX_OBJECT: u64 = 8 * 1024 * 1024;
/// Profondeur maximale d'une chaîne de deltas (Git utilise 50 par défaut).
const MAX_DELTA_DEPTH: usize = 64;

type Oid = [u8; 20];

const OBJ_COMMIT: u8 = 1;
const OBJ_OFS_DELTA: u8 = 6;
const OBJ_REF_DELTA: u8 = 7;

pub(super) fn read(root: &Path, count: usize) -> Option<GitInfo> {
    let repo = Repo::discover(root)?;
    let (branch, head) = repo.head();
    let subjects = head.map(|id| repo.recent_subjects(id, count)).unwrap_or_default();
    Some(GitInfo { branch, conventional_ratio: conventional_ratio(&subjects), recent_subjects: subjects })
}

/// Part des sujets de commit au format Conventional Commits.
pub(super) fn conventional_ratio(subjects: &[String]) -> f32 {
    if subjects.is_empty() {
        return 0.0;
    }
    let n = subjects.iter().filter(|s| CONVENTIONAL.is_match(s)).count();
    n as f32 / subjects.len() as f32
}

struct Repo {
    /// Dossier propre à l'arbre de travail (contient `HEAD`).
    git_dir: PathBuf,
    /// Dossier partagé (objets, références) ; différent pour un worktree.
    common_dir: PathBuf,
    packs: Vec<PackIndex>,
}

impl Repo {
    fn discover(root: &Path) -> Option<Self> {
        let dot_git = root.join(".git");
        let git_dir = if dot_git.is_dir() {
            dot_git
        } else {
            // Worktree ou sous-module : fichier `gitdir: <chemin>`.
            let text = fs::read_to_string(&dot_git).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim();
            root.join(target)
        };
        let common_dir = match fs::read_to_string(git_dir.join("commondir")) {
            Ok(rel) => git_dir.join(rel.trim()),
            Err(_) => git_dir.clone(),
        };
        let mut packs = Vec::new();
        if let Ok(entries) = fs::read_dir(common_dir.join("objects/pack")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "idx")
                    && let Some(pack) = PackIndex::open(&path)
                {
                    packs.push(pack);
                }
            }
        }
        Some(Self { git_dir, common_dir, packs })
    }

    /// Branche courante (absente si HEAD est détaché) et commit pointé.
    fn head(&self) -> (Option<String>, Option<Oid>) {
        let Ok(text) = fs::read_to_string(self.git_dir.join("HEAD")) else { return (None, None) };
        let text = text.trim();
        match text.strip_prefix("ref:") {
            Some(name) => {
                let name = name.trim();
                let branch = name.strip_prefix("refs/heads/").unwrap_or(name).to_owned();
                (Some(branch), self.resolve_ref(name, 0))
            }
            None => (None, parse_hex(text)),
        }
    }

    fn resolve_ref(&self, name: &str, depth: usize) -> Option<Oid> {
        if depth > 5 || name.contains("..") {
            return None;
        }
        for dir in [&self.git_dir, &self.common_dir] {
            if let Ok(text) = fs::read_to_string(dir.join(name)) {
                let text = text.trim();
                return match text.strip_prefix("ref:") {
                    Some(target) => self.resolve_ref(target.trim(), depth + 1),
                    None => parse_hex(text),
                };
            }
        }
        let packed = fs::read_to_string(self.common_dir.join("packed-refs")).ok()?;
        packed
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with('^'))
            .find_map(|l| l.split_once(' ').filter(|(_, r)| *r == name).and_then(|(id, _)| parse_hex(id)))
    }

    /// Sujets des `count` derniers commits (hors fusions), du plus récent au
    /// plus ancien selon la date de commit, comme `git log`.
    fn recent_subjects(&self, head: Oid, count: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut queue = BinaryHeap::new();
        let mut seen = HashSet::new();
        let mut visited = 0;
        if let Some(commit) = self.commit(head) {
            seen.insert(head);
            queue.push(commit);
        }
        // Borne sur le nombre de commits parcourus, fusions comprises.
        while let Some(commit) = queue.pop() {
            visited += 1;
            if out.len() >= count || visited > count * 4 {
                break;
            }
            if commit.parents.len() <= 1 && !commit.subject.is_empty() {
                out.push(commit.subject.clone());
            }
            for parent in &commit.parents {
                if seen.insert(*parent)
                    && let Some(c) = self.commit(*parent)
                {
                    queue.push(c);
                }
            }
        }
        out
    }

    fn commit(&self, id: Oid) -> Option<Commit> {
        let (kind, data) = self.object(id)?;
        (kind == OBJ_COMMIT).then(|| Commit::parse(&data))
    }

    fn object(&self, id: Oid) -> Option<(u8, Vec<u8>)> {
        self.loose_object(id).or_else(|| self.packed_object(id, 0))
    }

    fn loose_object(&self, id: Oid) -> Option<(u8, Vec<u8>)> {
        let hex = to_hex(&id);
        let path = self.common_dir.join("objects").join(&hex[..2]).join(&hex[2..]);
        let file = File::open(path).ok()?;
        let mut data = Vec::new();
        ZlibDecoder::new(file).take(MAX_OBJECT).read_to_end(&mut data).ok()?;
        let nul = data.iter().position(|b| *b == 0)?;
        let header = std::str::from_utf8(&data[..nul]).ok()?;
        let kind = match header.split(' ').next()? {
            "commit" => OBJ_COMMIT,
            "tree" => 2,
            "blob" => 3,
            "tag" => 4,
            _ => return None,
        };
        Some((kind, data[nul + 1..].to_vec()))
    }

    fn packed_object(&self, id: Oid, depth: usize) -> Option<(u8, Vec<u8>)> {
        self.packs.iter().find_map(|pack| {
            let offset = pack.find(&id)?;
            self.read_packed_at(pack, offset, depth)
        })
    }

    fn read_packed_at(&self, pack: &PackIndex, offset: u64, depth: usize) -> Option<(u8, Vec<u8>)> {
        if depth > MAX_DELTA_DEPTH {
            return None;
        }
        let mut reader = BufReader::new(File::open(&pack.pack_path).ok()?);
        reader.seek(SeekFrom::Start(offset)).ok()?;
        let (kind, size) = read_entry_header(&mut reader)?;
        match kind {
            OBJ_OFS_DELTA => {
                let distance = read_offset_delta(&mut reader)?;
                let base_offset = offset.checked_sub(distance)?;
                let delta = inflate(&mut reader, size)?;
                let (base_kind, base) = self.read_packed_at(pack, base_offset, depth + 1)?;
                Some((base_kind, apply_delta(&base, &delta)?))
            }
            OBJ_REF_DELTA => {
                let mut base_id = [0u8; 20];
                reader.read_exact(&mut base_id).ok()?;
                let delta = inflate(&mut reader, size)?;
                let (base_kind, base) =
                    self.loose_object(base_id).or_else(|| self.packed_object(base_id, depth + 1))?;
                Some((base_kind, apply_delta(&base, &delta)?))
            }
            1..=4 => Some((kind, inflate(&mut reader, size)?)),
            _ => None,
        }
    }
}

/// Commit, ordonné par date de commit pour le parcours.
#[derive(Debug, PartialEq, Eq)]
struct Commit {
    time: i64,
    parents: Vec<Oid>,
    subject: String,
}

impl Ord for Commit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time).then_with(|| self.subject.cmp(&other.subject))
    }
}

impl PartialOrd for Commit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Commit {
    fn parse(data: &[u8]) -> Self {
        let text = String::from_utf8_lossy(data);
        let (headers, message) = text.split_once("\n\n").unwrap_or((&text, ""));
        let mut parents = Vec::new();
        let mut time = 0;
        for line in headers.lines() {
            if let Some(id) = line.strip_prefix("parent ") {
                parents.extend(parse_hex(id.trim()));
            } else if let Some(rest) = line.strip_prefix("committer ") {
                // « Nom <courriel> 1700000000 +0100 »
                time = rest.rsplit(' ').nth(1).and_then(|t| t.parse().ok()).unwrap_or(0);
            }
        }
        let subject = message.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_owned();
        Self { time, parents, subject }
    }
}

/// Index de paquet v2, lu à la demande (recherche dichotomique sur disque).
struct PackIndex {
    idx_path: PathBuf,
    pack_path: PathBuf,
    fanout: Vec<u32>,
}

impl PackIndex {
    fn open(idx_path: &Path) -> Option<Self> {
        let mut file = File::open(idx_path).ok()?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;
        if header != [0xff, b't', b'O', b'c', 0, 0, 0, 2] {
            return None;
        }
        let mut raw = vec![0u8; 256 * 4];
        file.read_exact(&mut raw).ok()?;
        let fanout = raw.chunks_exact(4).map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]])).collect();
        Some(Self { idx_path: idx_path.to_path_buf(), pack_path: idx_path.with_extension("pack"), fanout })
    }

    fn find(&self, id: &Oid) -> Option<u64> {
        let total = u64::from(*self.fanout.last()?);
        let first = id[0] as usize;
        let mut lo = if first == 0 { 0 } else { u64::from(self.fanout[first - 1]) };
        let mut hi = u64::from(self.fanout[first]);
        let mut file = File::open(&self.idx_path).ok()?;
        let ids_start = 8 + 256 * 4;
        let mut candidate = [0u8; 20];
        while lo < hi {
            let mid = (lo + hi) / 2;
            file.seek(SeekFrom::Start(ids_start + mid * 20)).ok()?;
            file.read_exact(&mut candidate).ok()?;
            match candidate.cmp(id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    // Après les identifiants : CRC32 (4 o), puis décalages (4 o).
                    let offsets_start = ids_start + total * 24;
                    let mut word = [0u8; 4];
                    file.seek(SeekFrom::Start(offsets_start + mid * 4)).ok()?;
                    file.read_exact(&mut word).ok()?;
                    let small = u32::from_be_bytes(word);
                    if small & 0x8000_0000 == 0 {
                        return Some(u64::from(small));
                    }
                    // Grand paquet (> 2 Go) : table de décalages sur 8 octets.
                    let large_index = u64::from(small & 0x7fff_ffff);
                    let mut big = [0u8; 8];
                    file.seek(SeekFrom::Start(offsets_start + total * 4 + large_index * 8)).ok()?;
                    file.read_exact(&mut big).ok()?;
                    return Some(u64::from_be_bytes(big));
                }
            }
        }
        None
    }
}

fn read_byte(reader: &mut impl Read) -> Option<u8> {
    let mut b = [0u8; 1];
    reader.read_exact(&mut b).ok()?;
    Some(b[0])
}

/// En-tête d'une entrée de paquet : type sur 3 bits, taille en varint.
fn read_entry_header(reader: &mut impl Read) -> Option<(u8, u64)> {
    let mut byte = read_byte(reader)?;
    let kind = (byte >> 4) & 0x07;
    let mut size = u64::from(byte & 0x0f);
    let mut shift = 4;
    while byte & 0x80 != 0 {
        byte = read_byte(reader)?;
        size |= u64::from(byte & 0x7f).checked_shl(shift)?;
        shift += 7;
    }
    Some((kind, size))
}

/// Distance vers la base d'un `OFS_DELTA` (encodage propre à Git).
fn read_offset_delta(reader: &mut impl Read) -> Option<u64> {
    let mut byte = read_byte(reader)?;
    let mut value = u64::from(byte & 0x7f);
    while byte & 0x80 != 0 {
        byte = read_byte(reader)?;
        value = value.checked_add(1)?.checked_shl(7)? | u64::from(byte & 0x7f);
    }
    Some(value)
}

fn inflate(reader: &mut impl Read, size: u64) -> Option<Vec<u8>> {
    if size > MAX_OBJECT {
        return None;
    }
    let mut out = Vec::with_capacity(size as usize);
    ZlibDecoder::new(reader).take(size).read_to_end(&mut out).ok()?;
    (out.len() as u64 == size).then_some(out)
}

fn delta_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f).checked_shl(shift)?;
        shift += 7;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
}

/// Applique un delta Git (instructions « copier depuis la base » / « insérer »).
fn apply_delta(base: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    let source_size = delta_varint(delta, &mut pos)?;
    let target_size = delta_varint(delta, &mut pos)?;
    if source_size != base.len() as u64 || target_size > MAX_OBJECT {
        return None;
    }
    let mut out = Vec::with_capacity(target_size as usize);
    while pos < delta.len() {
        let op = delta[pos];
        pos += 1;
        if op & 0x80 != 0 {
            let mut offset = 0usize;
            let mut size = 0usize;
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    offset |= usize::from(*delta.get(pos)?) << (8 * i);
                    pos += 1;
                }
            }
            for i in 0..3 {
                if op & (0x10 << i) != 0 {
                    size |= usize::from(*delta.get(pos)?) << (8 * i);
                    pos += 1;
                }
            }
            if size == 0 {
                size = 0x10000;
            }
            out.extend_from_slice(base.get(offset..offset.checked_add(size)?)?);
        } else if op != 0 {
            let len = usize::from(op);
            out.extend_from_slice(delta.get(pos..pos + len)?);
            pos += len;
        } else {
            return None;
        }
    }
    (out.len() as u64 == target_size).then_some(out)
}

fn parse_hex(text: &str) -> Option<Oid> {
    if text.len() != 40 {
        return None;
    }
    let mut id = [0u8; 20];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

fn to_hex(id: &Oid) -> String {
    hf_net::to_hex(id)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn commit_body(parents: &[Oid], time: i64, message: &str) -> Vec<u8> {
        let mut s = format!("tree {}\n", "0".repeat(40));
        for p in parents {
            s.push_str(&format!("parent {}\n", to_hex(p)));
        }
        s.push_str(&format!("author A <a@x> {time} +0100\ncommitter A <a@x> {time} +0100\n\n{message}\n"));
        s.into_bytes()
    }

    fn write_loose(git: &Path, id: Oid, body: &[u8]) {
        let hex = to_hex(&id);
        let dir = git.join("objects").join(&hex[..2]);
        fs::create_dir_all(&dir).unwrap();
        let mut raw = format!("commit {}\0", body.len()).into_bytes();
        raw.extend_from_slice(body);
        fs::write(dir.join(&hex[2..]), zlib(&raw)).unwrap();
    }

    #[test]
    fn conventional_commit_ratio() {
        let subjects: Vec<String> = ["feat(ui): add list", "fix: crash", "Update README", "chore!: drop node 18"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!((conventional_ratio(&subjects) - 0.75).abs() < f32::EPSILON);
        assert_eq!(conventional_ratio(&[]), 0.0);
    }

    #[test]
    fn loose_objects_branch_and_date_order() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir_all(git.join("refs/heads")).unwrap();
        let (a, b, c, m) = ([1u8; 20], [2u8; 20], [3u8; 20], [4u8; 20]);
        write_loose(&git, a, &commit_body(&[], 100, "feat: init"));
        write_loose(&git, b, &commit_body(&[a], 300, "fix: b est plus récent"));
        write_loose(&git, c, &commit_body(&[a], 200, "Ajout de c"));
        write_loose(&git, m, &commit_body(&[b, c], 400, "Merge branch 'c'"));
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(git.join("packed-refs"), format!("# pack-refs\n{} refs/heads/main\n", to_hex(&m))).unwrap();

        let info = read(dir.path(), 10).unwrap();
        assert_eq!(info.branch.as_deref(), Some("main"));
        // La fusion est ignorée, l'ordre suit la date de commit.
        assert_eq!(info.recent_subjects, vec!["fix: b est plus récent", "Ajout de c", "feat: init"]);
        assert_eq!(read(dir.path(), 1).unwrap().recent_subjects.len(), 1);
    }

    #[test]
    fn missing_objects_and_detached_head_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir_all(&git).unwrap();
        let (a, gone) = ([5u8; 20], [6u8; 20]);
        // Clone superficiel : le parent n'existe pas localement.
        write_loose(&git, a, &commit_body(&[gone], 10, "docs: seul commit"));
        fs::write(git.join("HEAD"), format!("{}\n", to_hex(&a))).unwrap();
        let info = read(dir.path(), 10).unwrap();
        assert_eq!(info.branch, None);
        assert_eq!(info.recent_subjects, vec!["docs: seul commit"]);
        assert!(read(&dir.path().join("absent"), 10).is_none());
    }

    /// Construit un paquet v2 : A complet, B en OFS_DELTA de A, C en REF_DELTA de B.
    #[test]
    fn packed_objects_with_deltas() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        let pack_dir = git.join("objects/pack");
        fs::create_dir_all(&pack_dir).unwrap();
        let (a, b, c) = ([0x10u8; 20], [0x20u8; 20], [0x30u8; 20]);
        let body_a = commit_body(&[], 1, "feat: a");
        let body_b = commit_body(&[a], 2, "fix: b");
        let body_c = commit_body(&[b], 3, "test: c");

        // Delta : copie de l'en-tête `tree …\n` (46 octets) puis insertion du reste.
        let delta = |base: &[u8], target: &[u8]| {
            let mut d = Vec::new();
            for mut n in [base.len(), target.len()] {
                loop {
                    let byte = (n & 0x7f) as u8;
                    n >>= 7;
                    d.push(if n > 0 { byte | 0x80 } else { byte });
                    if n == 0 {
                        break;
                    }
                }
            }
            d.extend_from_slice(&[0x80 | 0x10, 46]);
            for chunk in target[46..].chunks(127) {
                d.push(chunk.len() as u8);
                d.extend_from_slice(chunk);
            }
            d
        };
        let header = |kind: u8, mut size: usize| {
            let mut h = vec![(kind << 4) | (size & 0x0f) as u8];
            size >>= 4;
            while size > 0 {
                *h.last_mut().unwrap() |= 0x80;
                h.push((size & 0x7f) as u8);
                size >>= 7;
            }
            h
        };

        let mut pack = b"PACK\0\0\0\x02\0\0\0\x03".to_vec();
        let off_a = pack.len() as u32;
        pack.extend(header(OBJ_COMMIT, body_a.len()));
        pack.extend(zlib(&body_a));
        let off_b = pack.len() as u32;
        let d_ab = delta(&body_a, &body_b);
        pack.extend(header(OBJ_OFS_DELTA, d_ab.len()));
        let distance = off_b - off_a;
        assert!(distance < 128, "encodage sur un octet dans ce test");
        pack.push(distance as u8);
        pack.extend(zlib(&d_ab));
        let off_c = pack.len() as u32;
        let d_bc = delta(&body_b, &body_c);
        pack.extend(header(OBJ_REF_DELTA, d_bc.len()));
        pack.extend_from_slice(&b);
        pack.extend(zlib(&d_bc));
        fs::write(pack_dir.join("pack-test.pack"), &pack).unwrap();

        let mut entries = [(a, off_a), (b, off_b), (c, off_c)];
        entries.sort();
        let mut idx = vec![0xff, b't', b'O', b'c', 0, 0, 0, 2];
        for byte in 0..=255u8 {
            let n = entries.iter().filter(|(id, _)| id[0] <= byte).count() as u32;
            idx.extend(n.to_be_bytes());
        }
        entries.iter().for_each(|(id, _)| idx.extend_from_slice(id));
        entries.iter().for_each(|_| idx.extend([0u8; 4]));
        entries.iter().for_each(|(_, off)| idx.extend(off.to_be_bytes()));
        fs::write(pack_dir.join("pack-test.idx"), &idx).unwrap();
        fs::write(git.join("HEAD"), format!("{}\n", to_hex(&c))).unwrap();

        let info = read(dir.path(), 10).unwrap();
        assert_eq!(info.recent_subjects, vec!["test: c", "fix: b", "feat: a"]);
        assert!((info.conventional_ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn corrupt_delta_is_rejected() {
        let base = b"hello";
        // Taille source annoncée différente de la base.
        assert!(apply_delta(base, &[4, 5, 0x90, 5]).is_none());
        // Copie hors limites.
        assert!(apply_delta(base, &[5, 6, 0x90, 6]).is_none());
        assert_eq!(apply_delta(base, &[5, 7, 0x90, 5, 2, b'!', b'!']).unwrap(), b"hello!!");
    }

    /// Le dépôt de HarnessForge lui-même, quand les sources sont un clone Git.
    #[test]
    fn reads_this_repository_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if root.join(".git").exists() {
            let info = read(&root, 5).expect("dépôt lisible");
            assert!(!info.recent_subjects.is_empty(), "au moins le commit courant");
        }
    }
}
