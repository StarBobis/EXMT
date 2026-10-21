//! Differential driver for `skeleton::merge`.
//!
//! Reads a fixture `{tables:[{mesh,hashes,bindpose}], paths:{hash:path}}` and
//! prints the merged skeleton as JSON for comparison with Python.

use std::collections::HashMap;

use exmt_core::skeleton;
use serde_json::{Value, json};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: skeleton_cli <fixture.json>");
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let value: Value = serde_json::from_str(&text).expect("parse fixture");
    let tables: Vec<(String, Vec<u32>, Vec<f32>)> = value["tables"]
        .as_array()
        .expect("tables")
        .iter()
        .map(|row| {
            let mesh = row["mesh"].as_str().unwrap().to_string();
            let hashes: Vec<u32> = row["hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            let bindpose: Vec<f32> = row["bindpose"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            (mesh, hashes, bindpose)
        })
        .collect();
    let mut paths = HashMap::new();
    if let Some(map) = value["paths"].as_object() {
        for (key, v) in map {
            if let (Ok(hash), Some(name)) = (key.parse::<u32>(), v.as_str()) {
                paths.insert(hash, name.to_string());
            }
        }
    }
    let skel = skeleton::merge(&tables, &paths, 0.01).expect("merge");
    let bones: Vec<Value> = skel
        .bones
        .iter()
        .map(|b| {
            json!({
                "hash": b.hash,
                "name": b.name,
                "parent": b.parent,
                "rest": b.rest.to_vec(),
                "meshes": b.meshes,
            })
        })
        .collect();
    let local: serde_json::Map<String, Value> = skel
        .local
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    let out = json!({
        "bones": bones,
        "local": local,
        "named": skel.named,
        "conflicts": skel.conflicts.iter().map(|c| json!({
            "hash": c.hash, "mesh": c.mesh, "deviation": c.deviation
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}
