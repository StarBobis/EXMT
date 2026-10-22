//! Dump material -> texture links from a bundle, by PPtr (not by name).
//!
//! Usage: `cargo run --release --example dump_materials -- <bundle> [mesh_name]`
//!
//! Prints one line per Material object:
//! `<material>|<path_id>|env=Texture2DName,...`
//! and, when `mesh_name` is given, the renderer(s) whose `m_Mesh` points at that
//! mesh, with their material list resolved to names. This is the ground truth
//! for "which texture does this submesh actually use"; name heuristics are only
//! a fallback.

use std::collections::HashMap;
use std::process::ExitCode;

use exmt_core::typetree::Value;
use exmt_core::{container, serialized, typetree};

fn pptr_path_id(value: &Value) -> Option<i64> {
    match value {
        // Older/other trees may flatten the two ints into a pair.
        Value::Pair(_, path_id) => path_id.as_i64(),
        // Unity's tree spells PPtr as a struct with m_FileID / m_PathID.
        Value::Struct(_) => value.get("m_PathID").and_then(|v| v.as_i64()),
        _ => None,
    }
}

fn tex_env_textures(statics: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(envs) = statics
        .get("m_SavedProperties")
        .and_then(|sheet| sheet.get("m_TexEnvs"))
        .and_then(|envs| envs.as_array())
    else {
        return out;
    };
    for pair in envs {
        let Value::Pair(name, env) = pair else { continue };
        let Some(name) = name.as_str() else { continue };
        let Some(texture) = env.get("m_Texture") else { continue };
        let Some(path_id) = pptr_path_id(texture) else { continue };
        out.push((name.to_string(), path_id.to_string()));
    }
    out
}

fn run(bundle: &str, mesh_filter: Option<&str>) -> Result<String, String> {
    let raw = std::fs::read(bundle).map_err(|e| format!("read: {e}"))?;
    let plain = container::decrypt(&raw).map_err(|e| format!("decrypt: {e}"))?;
    let info = container::parse(&plain).map_err(|e| format!("container: {e}"))?;
    let nodes = container::node_payloads(&plain, &info).map_err(|e| format!("nodes: {e}"))?;

    let mut out = String::new();
    for (node_index, node) in nodes.iter().enumerate() {
        let Ok(sf) = serialized::SerializedFile::parse(&node.data) else {
            continue;
        };
        // path_id -> (class id, decoded tree); small enough for one character bundle.
        let mut objects: HashMap<i64, (i32, Value)> = HashMap::new();
        for (path_id, obj) in &sf.objects {
            let Some(ty) = sf.type_of(obj) else { continue };
            let Ok(value) = typetree::read_value(&sf, obj) else {
                continue;
            };
            objects.insert(*path_id, (ty.class_id, value));
        }

        let name_of = |path_id: i64| -> String {
            objects
                .get(&path_id)
                .and_then(|(_, tree)| {
                    tree.get("m_Name").and_then(|v| v.as_str()).map(str::to_string)
                })
                .unwrap_or_else(|| format!("<object {path_id}>"))
        };

        if std::env::var_os("EXMT_DUMP_CLASSES").is_some() {
            for (index, node) in nodes.iter().enumerate() {
                out.push_str(&format!("#node|{index}|{}\n", node.path));
            }
            for (index, external) in sf.externals.iter().enumerate() {
                out.push_str(&format!(
                    "#external|node{node_index}|{index}|{}\n",
                    external.path
                ));
            }
            let mut histogram: std::collections::BTreeMap<i32, usize> = std::collections::BTreeMap::new();
            for (_, (class_id, _)) in &objects {
                *histogram.entry(*class_id).or_default() += 1;
            }
            let summary: Vec<String> = histogram
                .iter()
                .map(|(class_id, count)| format!("{class_id}:{count}"))
                .collect();
            out.push_str(&format!(
                "#classes|node{node_index}|file={}|{}\n",
                sf.objects.len(),
                summary.join(",")
            ));
        }

        // AssetBundle (class 142): container names and dependency bundles.
        for (path_id, (class_id, tree)) in &objects {
            if *class_id != 142 {
                continue;
            }
            let name = tree.get("m_Name").and_then(|v| v.as_str()).unwrap_or("");
            let deps: Vec<String> = tree
                .get("m_Dependencies")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let keys: Vec<String> = tree
                .get("m_Container")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| match item {
                            Value::Pair(key, _) => key.as_str().map(str::to_string),
                            _ => None,
                        })
                        .take(30)
                        .collect()
                })
                .unwrap_or_default();
            out.push_str(&format!(
                "assetbundle|{path_id}|{name}|deps={}|keys={}\n",
                deps.join(","),
                keys.join(",")
            ));
        }

        // Material = class 21. m_SavedProperties.m_TexEnvs is the texture slot table.
        for (path_id, (class_id, tree)) in &objects {
            if *class_id != 21 {
                continue;
            }
            let material = tree.get("m_Name").and_then(|v| v.as_str()).unwrap_or("");
            let envs = tex_env_textures(tree);
            let slots: Vec<String> = envs
                .iter()
                .map(|(env, texture_id)| {
                    let texture = texture_id
                        .parse::<i64>()
                        .ok()
                        .map(&name_of)
                        .unwrap_or_else(|| format!("<pptr {texture_id}>"));
                    format!("{env}={texture}")
                })
                .collect();
            out.push_str(&format!("{material}|{path_id}|{}\n", slots.join(",")));
        }

        // Renderer (MeshRenderer 23 / SkinnedMeshRenderer 137): m_Materials -> names.
        if let Some(mesh_name) = mesh_filter {
            let mesh_id = objects.iter().find_map(|(path_id, (class_id, tree))| {
                if *class_id == 43
                    && tree.get("m_Name").and_then(|v| v.as_str()) == Some(mesh_name)
                {
                    Some(*path_id)
                } else {
                    None
                }
            });
            let Some(mesh_id) = mesh_id else {
                continue;
            };
            for (path_id, (class_id, tree)) in &objects {
                if *class_id != 137 && *class_id != 23 {
                    continue;
                }
                let target = tree.get("m_Mesh").and_then(pptr_path_id);
                if target != Some(mesh_id) {
                    continue;
                }
                let materials: Vec<String> = tree
                    .get("m_Materials")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                pptr_path_id(item)
                                    .map(&name_of)
                                    .unwrap_or_else(|| "<none>".to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push_str(&format!(
                    "renderer|node{node_index}|{path_id}|class={class_id}|mesh={mesh_name}|materials={}\n",
                    materials.join(",")
                ));
            }
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: {} <bundle> [mesh_name]", args[0]);
        return ExitCode::FAILURE;
    }
    match run(&args[1], args.get(2).map(String::as_str)) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
