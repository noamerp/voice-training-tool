use rand::seq::SliceRandom;
use std::env;
use std::fs;
use std::path::Path;

#[derive(serde::Deserialize)]
struct GithubEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // --- Harvard sentences ---
    let dest = Path::new(&out_dir).join("harvard_sentences.rs");
    let text = ureq::get(
        "https://www.cs.cmu.edu/afs/cs.cmu.edu/project/fgdata/OldFiles/Recorder.app/utterances/Type1/harvsents.txt",
    )
    .call()
    .expect("failed to download Harvard sentences")
    .body_mut()
    .read_to_string()
    .expect("failed to read response body");

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

    // --- Common Voice sentences ---
    let mut rng = rand::rng();

    let json_text =
        ureq::get("https://api.github.com/repos/common-voice/common-voice/contents/server/data")
            .header("User-Agent", "voice-training-tool-build/1.0")
            .call()
            .expect("failed to fetch Common Voice language list")
            .body_mut()
            .read_to_string()
            .expect("failed to read language list response");

    let entries: Vec<GithubEntry> =
        serde_json::from_str(&json_text).expect("failed to parse Common Voice language list");

    let langs: Vec<String> = entries
        .into_iter()
        .filter(|e| e.kind == "dir")
        .map(|e| e.name)
        .collect();

    let mut cv_data: Vec<(String, Vec<String>)> = Vec::new();

    for lang in &langs {
        let url = format!(
            "https://raw.githubusercontent.com/common-voice/common-voice/main/server/data/{}/sentence-collector.txt",
            lang
        );
        let text = match ureq::get(&url).call() {
            Ok(mut resp) => match resp.body_mut().read_to_string() {
                Ok(t) => t,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let mut sentences: Vec<String> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();

        if sentences.len() <= 20 {
            continue;
        }

        sentences.shuffle(&mut rng);
        sentences.truncate(1000);

        cv_data.push((lang.clone(), sentences));
    }

    cv_data.sort_by(|a, b| a.0.cmp(&b.0));

    let mut cv_code = String::from("static COMMON_VOICE_SENTENCES: &[(&str, &[&str])] = &[\n");
    for (lang, sentences) in &cv_data {
        cv_code.push_str(&format!("    ({:?}, &[\n", lang));
        for s in sentences {
            cv_code.push_str(&format!("        {:?},\n", s));
        }
        cv_code.push_str("    ]),\n");
    }
    cv_code.push_str("];\n\n");

    cv_code.push_str("static COMMON_VOICE_LANGUAGES: &[&str] = &[\n");
    for (lang, _) in &cv_data {
        cv_code.push_str(&format!("    {:?},\n", lang));
    }
    cv_code.push_str("];\n");

    let dest = Path::new(&out_dir).join("common_voice_sentences.rs");
    fs::write(dest, cv_code).expect("failed to write common_voice_sentences.rs");

    println!("cargo:rerun-if-changed=build.rs");
}
