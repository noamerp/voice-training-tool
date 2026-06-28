use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("harvard_sentences.rs");

    let text = ureq::get(
        "https://www.cs.cmu.edu/afs/cs.cmu.edu/project/fgdata/OldFiles/Recorder.app/utterances/Type1/harvsents.txt",
    )
    .call()
    .expect("failed to download Harvard sentences")
    .into_string()
    .expect("failed to read response body");

    // Extract every numbered sentence line ("1. …" through "10. …")
    let sentences: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let dot = line.find(". ")?;
            line[..dot].trim().parse::<u32>().ok()?;
            Some(line[dot + 2..].to_string())
        })
        .collect();

    assert!(
        sentences.len() % 10 == 0 && !sentences.is_empty(),
        "unexpected sentence count: {}",
        sentences.len()
    );

    let mut code = String::from("static HARVARD_LISTS: &[&[&str]] = &[\n");
    for chunk in sentences.chunks(10) {
        code.push_str("    &[\n");
        for s in chunk {
            code.push_str(&format!("        {:?},\n", s));
        }
        code.push_str("    ],\n");
    }
    code.push_str("];\n");

    fs::write(dest, code).expect("failed to write harvard_sentences.rs");

    println!("cargo:rerun-if-changed=build.rs");
}
