//! Name-based pairing of a skin's submeshes with its texture families.
//!
//! Ported from `core/exmt_core/pairings.py`. Materials are not available
//! offline, so the game's naming convention is the only evidence: a texture
//! family (`…_slg_cloth1_d/n/rmo`) belongs to the mesh whose part token matches
//! strongest. The rules are deliberately layered (direct, numbered, head
//! groups, siblings, shared props, body atlas) because no single prefix test
//! survives the shipped names.

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub name: String,
    pub prefix: String,
    pub character: String,
    pub tokens: Vec<String>,
    pub head: String,
    pub role: String,
    pub stem: String,
    pub base: String,
}

const ROLES: [&str; 16] = [
    "d", "da", "dif", "dt", "bt", "nt", "n", "rmo", "spc", "smo", "smoe", "h", "ramp",
    "mask", "il", "ds",
];

const NOISE: [&str; 14] = [
    "dorm", "fight", "general", "flat", "bend", "trans", "vfx", "swap", "wt", "uber",
    "ubertrans", "mat", "pearl", "drom",
];

const BASE_BODY: [&str; 3] = ["B_body", "new_body", "body"];

const SHARED_NS: [&str; 14] = [
    "tongyong", "none", "aelg", "cf", "elid", "mecha", "motordriven", "br", "npc",
    "enemy", "prop", "item", "weapon", "handmade",
];

const DIFFUSE: [&str; 4] = ["d", "da", "dif", "bt"];

fn is_role(token: &str) -> bool {
    let low = token.to_ascii_lowercase();
    ROLES.iter().any(|role| *role == low)
}

fn is_lod(token: &str) -> bool {
    let low = token.to_ascii_lowercase();
    let rest = low.strip_prefix('l').unwrap_or(&low);
    let Some(rest) = rest.strip_prefix("od") else { return false };
    let rest = rest.strip_prefix('m').unwrap_or(rest);
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

fn is_noise(token: &str) -> bool {
    let low = token.to_ascii_lowercase();
    let trimmed = low.trim_end_matches(|c: char| c.is_ascii_digit());
    NOISE.iter().any(|word| *word == trimmed)
}

fn typo(token: &str) -> &str {
    match token.to_ascii_lowercase().as_str() {
        "boby" | "boddy" => "body",
        "hare" | "hiar" => "hair",
        "sanzi" => "shanzi",
        "glasses" => "glass",
        _ => token,
    }
}

fn tokens(name: &str) -> Vec<String> {
    name.split('_').filter(|tok| !tok.is_empty()).map(|tok| tok.to_string()).collect()
}

fn family_of(character: &str) -> String {
    let mut base = character.to_string();
    loop {
        let mut trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
        for suffix in ["ssr", "sr", "r", "dorm", "fight"] {
            if trimmed.to_ascii_lowercase().ends_with(suffix) {
                trimmed.truncate(trimmed.len() - suffix.len());
                break;
            }
        }
        if trimmed == base || trimmed.is_empty() {
            return if trimmed.is_empty() { base } else { trimmed };
        }
        base = trimmed;
    }
}

/// Parse one mesh or texture name; `None` when it is not a character asset.
pub fn parse(name: &str) -> Option<Info> {
    if name.is_empty() || !name.contains('_') {
        return None;
    }
    let mut raw = tokens(name);
    if raw.len() < 2 {
        return None;
    }
    let mut role = String::new();
    if raw.len() > 2 && is_role(raw.last().unwrap()) {
        role = raw.pop().unwrap().to_ascii_lowercase();
    }
    let prefix = raw[0].to_ascii_lowercase();
    if prefix != "c" && prefix != "cw" {
        return None;
    }
    if raw.len() < 2 {
        return None;
    }
    let character = raw[1].clone();
    let body: Vec<String> = raw[2..].iter().filter(|tok| !is_lod(tok)).cloned().collect();
    let mut semantic: Vec<String> = body
        .iter()
        .filter(|tok| !is_lod(tok) && !is_noise(tok))
        .map(|tok| typo(tok).to_string())
        .collect();
    let base = family_of(&character);
    if semantic.is_empty() {
        if prefix == "cw" {
            semantic = vec!["WL".into()];
        } else {
            return Some(Info {
                name: name.into(),
                prefix: prefix.clone(),
                character: character.clone(),
                tokens: Vec::new(),
                head: String::new(),
                role,
                stem: format!("{prefix}_{character}"),
                base,
            });
        }
    }
    let mut head = semantic.last().unwrap().to_ascii_lowercase();
    if prefix == "cw" && semantic.iter().any(|tok| tok.eq_ignore_ascii_case("wl")) {
        head = "wl".into();
    }
    let mut stem_parts = vec![prefix.clone(), character.clone()];
    stem_parts.extend(semantic.iter().cloned());
    Some(Info {
        name: name.into(),
        prefix,
        character,
        tokens: semantic,
        head,
        role,
        stem: stem_parts.join("_"),
        base,
    })
}

fn char_score(mesh: &Info, tex: &Info) -> i32 {
    if mesh.character == tex.character {
        return 3;
    }
    let a = &mesh.character;
    let b = &tex.character;
    if a.starts_with(b.as_str()) || b.starts_with(a.as_str()) {
        let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
        let tail = &long[short.len()..];
        if !tail.is_empty()
            && tail.chars().all(|c| c.is_ascii_alphanumeric())
            && short.chars().last().is_some_and(|c| c.is_ascii_alphabetic())
        {
            return 2;
        }
    }
    if !mesh.base.is_empty() && mesh.base == tex.base {
        return 1;
    }
    0
}

fn direct(mesh: &Info, tex: &Info) -> bool {
    mesh.stem.starts_with(&format!("{}_", tex.stem)) || tex.stem.starts_with(&format!("{}_", mesh.stem))
}

fn heads_for(mesh: &Info, mesh_heads: &HashSet<String>) -> Vec<String> {
    let group = match mesh.head.as_str() {
        "hair" => Some(vec!["hair", "face", "eye", "eyeblend", "eyelash", "head", "helmet"]),
        "head" => Some(vec!["head", "face", "eye", "eyeblend", "eyelash", "hair"]),
        _ => None,
    };
    match group {
        Some(group) => {
            let mut out = vec![mesh.head.clone()];
            for other in &group[1..] {
                if !mesh_heads.contains(*other) {
                    out.push((*other).to_string());
                }
            }
            out
        }
        None => vec![mesh.head.clone()],
    }
}

pub fn families_of<'a>(textures: impl Iterator<Item = &'a String>) -> BTreeMap<String, Vec<String>> {
    let mut families: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in textures {
        if let Some(info) = parse(name) {
            if !info.role.is_empty() {
                families.entry(info.stem).or_default().push(name.clone());
            }
        }
    }
    families
}

fn role_of(name: &str) -> String {
    if let Some(info) = parse(name) {
        if !info.role.is_empty() {
            return info.role;
        }
    }
    let low = name.to_ascii_lowercase();
    for role in ["d", "da", "dif", "n", "nt", "rmo", "spc", "h"] {
        if low.ends_with(&format!("_{role}")) {
            return role.into();
        }
    }
    String::new()
}

fn has_diffuse(names: &[String]) -> bool {
    names.iter().any(|name| DIFFUSE.contains(&role_of(name).as_str()))
}

pub fn diffuse_of(names: &[String]) -> Option<String> {
    let rank = |role: &str| match role {
        "d" => Some(0),
        "da" => Some(1),
        "dif" => Some(2),
        "bt" => Some(3),
        _ => None,
    };
    let mut best: Option<(usize, String)> = None;
    for name in names {
        let role = role_of(name);
        let Some(value) = rank(&role) else { continue };
        if best.as_ref().map(|(r, _)| value < *r).unwrap_or(true) {
            best = Some((value, name.clone()));
        }
    }
    best.map(|(_, name)| name)
}

fn split_numeric_tail(text: &str) -> Option<(String, i32)> {
    let cut = text.trim_end_matches(|c: char| c.is_ascii_digit());
    if cut.len() == text.len() || cut.is_empty() {
        return None;
    }
    let number = text[cut.len()..].parse().ok()?;
    Some((cut.to_string(), number))
}

fn is_generic_head(head: &str) -> bool {
    let low = head.to_ascii_lowercase();
    for base in [
        "body", "hair", "face", "eye", "eyebrow", "eyeblend", "eyelash", "head", "hip",
        "cloth", "hand", "leg", "foot", "weapon", "slg", "wl",
    ] {
        if low == base {
            return true;
        }
        if let Some(rest) = low.strip_prefix(base) {
            if rest.starts_with(|c: char| c.is_ascii_digit()) || rest.starts_with('_') {
                if rest.chars().all(|c| c.is_ascii_digit() || c == '_') {
                    return true;
                }
            }
        }
    }
    if low.len() >= 2 && (low.starts_with('p') || low.starts_with('g')) {
        let rest = &low[1..];
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn variant_tail_token(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else { return false };
    if !first.eq_ignore_ascii_case(&'g') && !first.eq_ignore_ascii_case(&'p') {
        return false;
    }
    let rest = chars.as_str();
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

#[derive(Clone)]
struct Prop {
    names: Vec<String>,
    keys: HashSet<String>,
    character: String,
    prefixed: bool,
    shared: bool,
}

fn ordered_families<'a>(
    families: &'a BTreeMap<String, Vec<String>>,
    order: &[String],
) -> Vec<(String, Vec<String>)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in order {
        let Some(info) = parse(name) else { continue };
        if info.role.is_empty() || !seen.insert(info.stem.clone()) {
            continue;
        }
        if let Some(names) = families.get(&info.stem) {
            out.push((info.stem.clone(), names.clone()));
        }
    }
    for (stem, names) in families {
        if seen.insert(stem.clone()) {
            out.push((stem.clone(), names.clone()));
        }
    }
    out
}

fn prop_families(all_names: &[String]) -> BTreeMap<String, Prop> {
    let mut out: BTreeMap<String, Prop> = BTreeMap::new();
    for name in all_names {
        let low = name.to_ascii_lowercase();
        let mut stem = String::new();
        for role in ["d", "da", "dif", "bt", "n", "nt", "rmo", "spc", "h"] {
            let suffix = format!("_{role}");
            if low.ends_with(&suffix) {
                stem = name[..name.len() - suffix.len()].to_string();
                break;
            }
        }
        if stem.is_empty() {
            continue;
        }
        let toks: Vec<String> = stem.split('_').filter(|tok| !tok.is_empty() && !is_lod(tok)).map(|s| s.to_string()).collect();
        if toks.is_empty() {
            continue;
        }
        let prefixed = toks[0] == "c" || toks[0] == "cw";
        let character = if prefixed && toks.len() > 1 { toks[1].clone() } else { toks[0].clone() };
        let rest = if prefixed { &toks[2..] } else { &toks[1..] };
        let shared = SHARED_NS.contains(&character.to_ascii_lowercase().as_str())
            || rest.iter().any(|tok| SHARED_NS.contains(&tok.to_ascii_lowercase().as_str()));
        let entry = out.entry(stem).or_insert_with(|| Prop {
            names: Vec::new(),
            keys: HashSet::new(),
            character: character.clone(),
            prefixed,
            shared,
        });
        entry.names.push(name.clone());
        for tok in std::iter::once(&character).chain(rest.iter()) {
            let low = tok.to_ascii_lowercase();
            entry.keys.insert(low.clone());
            entry.keys.insert(low.trim_end_matches(|c: char| c.is_ascii_digit()).to_string());
        }
    }
    out
}

/// The game's texture families, computed once and reused for every skin.
///
/// Deriving them from all ~47,000 texture names inside `pair` meant every skin
/// re-parsed the whole catalogue. That work dominated the pack phase while its
/// input never changed.
pub struct Catalog {
    families: BTreeMap<String, Vec<String>>,
    ordered: Vec<(String, Vec<String>)>,
    all_names: Vec<String>,
    props: std::sync::OnceLock<BTreeMap<String, Prop>>,
}

impl Catalog {
    /// Families in texture-name order (`names.json`); the order decides ties.
    pub fn new(textures: &BTreeMap<String, Vec<String>>, order: &[String]) -> Self {
        let families = if order.is_empty() {
            families_of(textures.keys())
        } else {
            families_of(order.iter())
        };
        Self::from_families(families, textures, order)
    }

    fn from_families(
        families: BTreeMap<String, Vec<String>>,
        textures: &BTreeMap<String, Vec<String>>,
        order: &[String],
    ) -> Self {
        let ordered = ordered_families(&families, order);
        let mut all_names: Vec<String> =
            ordered.iter().flat_map(|(_, names)| names.iter().cloned()).collect();
        if all_names.is_empty() {
            all_names = textures.keys().cloned().collect();
        }
        Catalog {
            families,
            ordered,
            all_names,
            props: std::sync::OnceLock::new(),
        }
    }

    fn props(&self) -> &BTreeMap<String, Prop> {
        self.props.get_or_init(|| prop_families(&self.all_names))
    }

    /// `{mesh name: [texture names]}` for one skin, strongest evidence first.
    pub fn pair(&self, mesh_names: &[String]) -> BTreeMap<String, Vec<String>> {
        pair_with(mesh_names, self)
    }
}

/// `{mesh name: [texture names]}` for one skin, strongest evidence first.
pub fn pair(
    mesh_names: &[String],
    textures: &BTreeMap<String, Vec<String>>,
    families: Option<&BTreeMap<String, Vec<String>>>,
    order: &[String],
) -> BTreeMap<String, Vec<String>> {
    let families = match families {
        Some(families) => families.clone(),
        None => Catalog::new(textures, order).families,
    };
    let catalog = Catalog::from_families(families, textures, order);
    catalog.pair(mesh_names)
}

fn pair_with(mesh_names: &[String], catalog: &Catalog) -> BTreeMap<String, Vec<String>> {
    let meshes: Vec<Info> = mesh_names.iter().filter_map(|name| parse(name)).collect();
    if meshes.is_empty() {
        return BTreeMap::new();
    }
    let families = &catalog.families;
    let ordered = &catalog.ordered;
    let all_names = &catalog.all_names;
    let mesh_heads: HashSet<String> = meshes.iter().map(|m| m.head.clone()).collect();

    let mut winners: HashMap<String, Vec<(String, (i32, i32, i32, i32, String))>> = meshes
        .iter()
        .map(|m| (m.name.clone(), Vec::new()))
        .collect();
    for (stem, names) in ordered {
        let Some(info) = names.first().and_then(|name| parse(name)) else { continue };
        let head = info.head.clone();
        let diffuse = i32::from(has_diffuse(names));
        for mesh in &meshes {
            let strength = char_score(mesh, &info);
            if strength == 0 {
                continue;
            }
            let chain = i32::from(mesh.tokens == info.tokens && mesh.prefix == info.prefix);
            let score = if mesh.head == head {
                (if chain == 1 { 200 } else { 100 }) + strength * 10
            } else if mesh.head.is_empty() && head == "slg" {
                40 + strength * 10
            } else if direct(mesh, &info) {
                70 + strength * 10
            } else if !mesh.head.is_empty()
                && head.starts_with(&mesh.head)
                && head[mesh.head.len()..].chars().all(|c| c.is_ascii_digit())
                && !head[mesh.head.len()..].is_empty()
            {
                60 + strength * 10
            } else if !mesh.head.is_empty()
                && mesh.head.starts_with(&head)
                && mesh.head[head.len()..].chars().all(|c| c.is_ascii_digit())
                && !mesh.head[head.len()..].is_empty()
            {
                55 + strength * 10
            } else if heads_for(mesh, &mesh_heads).contains(&head) {
                50 + strength * 10
            } else {
                continue;
            };
            let list = winners.get_mut(&mesh.name).unwrap();
            if let Some((_, entry)) = list.iter_mut().find(|(key, _)| key == &head) {
                if (diffuse, score, chain, strength) > (entry.0, entry.1, entry.2, entry.3) {
                    *entry = (diffuse, score, chain, strength, stem.clone());
                }
            } else {
                list.push((
                    head.clone(),
                    (diffuse, score, chain, strength, stem.clone()),
                ));
            }
        }
    }

    let mut out: BTreeMap<String, Vec<String>> =
        meshes.iter().map(|m| (m.name.clone(), Vec::new())).collect();
    for mesh in &meshes {
        let Some(found) = winners.get_mut(&mesh.name) else { continue };
        // Vepley SSR01 的 cloth 曾因为 Dorm 的 `cloth` 头完全同名（strength 只有
        // 1）而拿走 Dorm 的整套贴图，3D 预览把另一套衣服的图集贴在这个网格的
        // UV 上。只有当**同变体**存在同名编号层（cloth01 / cloth02，strength 3）
        // 时，才让编号层压过别的变体的 exact head；其余情况保持原规则。
        let best_strength = found.iter().map(|(_, entry)| entry.3).max().unwrap_or(0);
        let has_strong_layer = found.iter().any(|(head, entry)| {
            entry.3 == best_strength
                && head.starts_with(&mesh.head)
                && head.len() > mesh.head.len()
                && head[mesh.head.len()..]
                    .chars()
                    .all(|c| c.is_ascii_digit())
        });
        let exact_head_strength = found
            .iter()
            .find(|(head, _)| head == &mesh.head)
            .map(|(_, entry)| entry.3);
        let has_head = match exact_head_strength {
            // 编号层才是同变体的真实贴图时，别让弱变体的同名家族独占
            Some(strength) => !has_strong_layer || strength == best_strength,
            None => false,
        };
        if has_head {
            let mesh_head = mesh.head.clone();
            found.retain(|(head, _)| {
                if head == &mesh_head {
                    return true;
                }
                let tail = if mesh_head.starts_with(head) { &mesh_head[head.len()..] } else { "" };
                let ext = if head.starts_with(&mesh_head) { &head[mesh_head.len()..] } else { "" };
                !((tail.starts_with(|c: char| c.is_ascii_digit()) && tail.chars().all(|c| c.is_ascii_digit()))
                    || (ext.starts_with(|c: char| c.is_ascii_digit()) && ext.chars().all(|c| c.is_ascii_digit())))
            });
        } else if has_strong_layer {
            // 同变体编号分层：保留整组（网格可能有多个材质槽），但把编号最大的
            // 放前面，`diffuse_of` 取到主图。Vepley SSR01 实测默认外观是
            // cloth02（白蓝制服），cloth01 是另一层。
            found.retain(|(_, entry)| entry.3 == best_strength);
            found.sort_by_key(|(head, _)| {
                let tail = head.strip_prefix(&mesh.head).unwrap_or("");
                (0i32, -tail.parse::<i32>().unwrap_or(0))
            });
        }
        for (_, (_, _, _, _, stem)) in found.iter() {
            if let Some(names) = families.get(stem) {
                out.get_mut(&mesh.name).unwrap().extend(names.clone());
            }
        }
    }

    // sibling submeshes: hand2 mirrors hand1, hip3 mirrors hip1
    for mesh in &meshes {
        if has_diffuse(&out[&mesh.name]) {
            continue;
        }
        let Some((base_head, _)) = split_numeric_tail(&mesh.head) else { continue };
        let mut best: Option<((i32, i32), String)> = None;
        for (stem, names) in ordered {
            let Some(info) = names.first().and_then(|name| parse(name)) else { continue };
            let Some((tex_base, number)) = split_numeric_tail(&info.head) else { continue };
            if tex_base != base_head || info.tokens[..info.tokens.len() - 1] != mesh.tokens[..mesh.tokens.len() - 1] {
                continue;
            }
            if char_score(mesh, &info) == 0 || !has_diffuse(names) {
                continue;
            }
            let key = (number, -char_score(mesh, &info));
            if best.as_ref().map(|(k, _)| key < *k).unwrap_or(true) {
                best = Some((key, stem.clone()));
            }
        }
        if let Some((_, stem)) = best {
            out.get_mut(&mesh.name).unwrap().extend(families[&stem].clone());
        }
    }

    // shared hand props: exact part token, never another skin's body/cloth
    let unresolved: Vec<&Info> = meshes.iter().filter(|m| !has_diffuse(&out[&m.name])).collect();
    if !unresolved.is_empty() {
        let props = catalog.props();
        for mesh in unresolved {
            let head = &mesh.head;
            if head.len() < 3 || is_generic_head(head) {
                continue;
            }
            let stem_head = head.trim_end_matches(|c: char| c.is_ascii_digit());
            let mut best: Option<((i32, usize, String), String)> = None;
            for (stem, entry) in props {
                if !entry.keys.contains(head) && !entry.keys.contains(stem_head) {
                    continue;
                }
                let rank = if entry.character.eq_ignore_ascii_case(head) {
                    0
                } else if entry.character.eq_ignore_ascii_case(&mesh.character) {
                    1
                } else if !entry.prefixed {
                    2
                } else if entry.shared {
                    3
                } else {
                    continue;
                };
                let key = (rank, stem.len(), stem.clone());
                if best.as_ref().map(|(k, _)| key < *k).unwrap_or(true) {
                    best = Some((key, stem.clone()));
                }
            }
            if let Some((_, stem)) = best {
                out.get_mut(&mesh.name).unwrap().extend(props[&stem].names.clone());
            }
        }
    }

    // fanOpen / fanShut are states of the same fan family
    for mesh in &meshes {
        if has_diffuse(&out[&mesh.name]) {
            continue;
        }
        let mut best: Option<((i32, i32, usize), String)> = None;
        for (stem, names) in ordered {
            let Some(info) = names.first().and_then(|name| parse(name)) else { continue };
            if info.head.is_empty() || !mesh.head.starts_with(&info.head) {
                continue;
            }
            let tail = &mesh.head[info.head.len()..];
            if tail.len() < 3 || !tail.chars().all(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            if char_score(mesh, &info) == 0 || !has_diffuse(names) {
                continue;
            }
            let key = (-char_score(mesh, &info), -(info.head.len() as i32), stem.len());
            if best.as_ref().map(|(k, _)| key < *k).unwrap_or(true) {
                best = Some((key, stem.clone()));
            }
        }
        if let Some((_, stem)) = best {
            out.get_mut(&mesh.name).unwrap().extend(families[&stem].clone());
        }
    }

    // colour variants: …_slg_G1 / _P2 vs numbered atlases …_01 / _02
    for mesh in &meshes {
        if has_diffuse(&out[&mesh.name]) {
            continue;
        }
        let Some(marker) = mesh.tokens.iter().position(|tok| variant_tail_token(tok)) else { continue };
        let mut prefix = mesh.tokens[..marker].to_vec();
        prefix.extend_from_slice(&mesh.tokens[marker + 1..]);
        if prefix.is_empty() {
            continue;
        }
        let mut best: Option<((i32, usize), String)> = None;
        for (stem, names) in ordered {
            let Some(info) = names.first().and_then(|name| parse(name)) else { continue };
            if info.tokens.len() < 2 || info.tokens[..info.tokens.len() - 1] != prefix {
                continue;
            }
            let last = info.tokens.last().unwrap();
            if last.parse::<i32>().is_err() || char_score(mesh, &info) == 0 || !has_diffuse(names) {
                continue;
            }
            let key = (last.parse().unwrap(), stem.len());
            if best.as_ref().map(|(k, _)| key < *k).unwrap_or(true) {
                best = Some((key, stem.clone()));
            }
        }
        if let Some((_, stem)) = best {
            out.get_mut(&mesh.name).unwrap().extend(families[&stem].clone());
        }
    }

    // shared body atlas when a body has no base-colour map of its own
    for mesh in &meshes {
        if !mesh.head.starts_with("body") || has_diffuse(&out[&mesh.name]) {
            continue;
        }
        for family in BASE_BODY {
            let picked: Vec<String> = all_names
                .iter()
                .filter(|name| {
                    name.starts_with(&format!("{family}_"))
                        && BASE_ROLE.iter().any(|role| name.to_ascii_lowercase().ends_with(&format!("_{role}")))
                })
                .cloned()
                .collect();
            if !picked.is_empty() {
                out.get_mut(&mesh.name).unwrap().extend(picked);
                break;
            }
        }
    }
    out
}

const BASE_ROLE: [&str; 8] = ["d", "da", "dif", "n", "nt", "rmo", "spc", "h"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_part_and_role() {
        let info = parse("c_AlvaSSR0102_slg_cloth1_lod0_d").unwrap();
        assert_eq!(info.prefix, "c");
        assert_eq!(info.character, "AlvaSSR0102");
        assert_eq!(info.head, "cloth1");
        assert_eq!(info.role, "d");
        assert_eq!(info.stem, "c_AlvaSSR0102_slg_cloth1");
        assert_eq!(info.base, "Alva");
    }

    #[test]
    fn variant_tail_is_stripped_for_family_names() {
        assert_eq!(family_of("BalthildeSSR01"), "Balthilde");
        assert_eq!(family_of("AgentDorm"), "Agent");
        assert_eq!(family_of("AGL01"), "AGL");
    }

    #[test]
    fn pairs_a_mesh_with_its_part_family_and_diffuse() {
        let textures = BTreeMap::from([
            ("c_A_slg_cloth1_d".to_string(), vec!["b".to_string()]),
            ("c_A_slg_cloth1_n".to_string(), vec!["b".to_string()]),
            ("c_A_slg_body_d".to_string(), vec!["b".to_string()]),
        ]);
        let groups = pair(
            &["c_A_slg_cloth1_lod0".to_string(), "c_A_slg_body_lod0".to_string()],
            &textures,
            None,
            &[],
        );
        assert!(groups["c_A_slg_cloth1_lod0"].contains(&"c_A_slg_cloth1_d".to_string()));
        assert_eq!(diffuse_of(&groups["c_A_slg_cloth1_lod0"]).as_deref(), Some("c_A_slg_cloth1_d"));
    }

    #[test]
    fn debug_elid_tie() {
        let textures = BTreeMap::from([
            ("c_ELID_VariantBeasteel_tree_slg_da".to_string(), vec!["b".to_string()]),
            ("c_ELID_VariantBeasteel_tree_slg_n".to_string(), vec!["b".to_string()]),
            ("c_ELID_VariantBeasteel_Body_slg_d".to_string(), vec!["b".to_string()]),
        ]);
        for name in textures.keys() {
            println!("parse {name}: {:?}", parse(name));
        }
        println!("mesh {:?}", parse("c_ELID_VariantBeasteel_Head_slg_lod0"));
        let groups = pair(&["c_ELID_VariantBeasteel_Head_slg_lod0".to_string()], &textures, None, &[
            "c_ELID_VariantBeasteel_tree_slg_da".to_string(),
            "c_ELID_VariantBeasteel_tree_slg_n".to_string(),
            "c_ELID_VariantBeasteel_Body_slg_d".to_string(),
        ]);
        println!("groups {groups:?}");
    }

    #[test]
    fn debug_mosinnagant() {
        let textures = BTreeMap::from([
            ("c_MosinnagantSSR01_slg_d".to_string(), vec!["b".to_string()]),
            ("c_MosinnagantSSR01_slg_n".to_string(), vec!["b".to_string()]),
            ("c_MosinnagantSSR01_slg_smo".to_string(), vec!["b".to_string()]),
        ]);
        println!("mesh {:?}", parse("c_MosinnagantSSR00_1_lod1"));
        println!("tex {:?}", parse("c_MosinnagantSSR01_slg_d"));
        let groups = pair(&["c_MosinnagantSSR00_1_lod1".to_string()], &textures, None, &[]);
        println!("groups {groups:?}");
    }

    #[test]
    fn same_variant_cloth_beats_another_skins_exact_head() {
        // Vepley SSR01: the Dorm cloth shares the exact head `cloth` but belongs
        // to a different outfit; the numbered same-character layers are the real
        // maps. Regression: the 3D preview used to paint this mesh with Dorm's
        // atlas (random UV islands all over the clothing).
        let textures = BTreeMap::from([
            ("c_VepleyDorm_slg_cloth_d".to_string(), vec!["b".to_string()]),
            ("c_VepleyDorm_slg_cloth_n".to_string(), vec!["b".to_string()]),
            ("c_VepleySSR01_slg_cloth01_d".to_string(), vec!["b".to_string()]),
            ("c_VepleySSR01_slg_cloth01_n".to_string(), vec!["b".to_string()]),
            ("c_VepleySSR01_slg_cloth02_d".to_string(), vec!["b".to_string()]),
            ("c_VepleySSR01_slg_cloth02_n".to_string(), vec!["b".to_string()]),
        ]);
        let groups = pair(
            &["c_VepleySSR01_slg_cloth_lod0".to_string()],
            &textures,
            None,
            &[],
        );
        let names = &groups["c_VepleySSR01_slg_cloth_lod0"];
        assert!(!names.iter().any(|name| name.contains("Dorm")), "{names:?}");
        assert_eq!(
            diffuse_of(names).as_deref(),
            Some("c_VepleySSR01_slg_cloth02_d"),
            "{names:?}"
        );
    }

    #[test]
    fn body_without_its_own_diffuse_uses_the_shared_atlas() {
        let textures = BTreeMap::from([
            ("B_body_d".to_string(), vec!["b".to_string()]),
            ("B_body_n".to_string(), vec!["b".to_string()]),
        ]);
        let groups = pair(&["c_A_slg_body_lod0".to_string()], &textures, None, &[]);
        assert_eq!(groups["c_A_slg_body_lod0"], vec!["B_body_d", "B_body_n"]);
    }
}



