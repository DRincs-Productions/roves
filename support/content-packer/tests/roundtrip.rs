use std::fs;
use std::path::{Path, PathBuf};

use roves_content_packer::extract::{self, ExtractOptions};
use roves_content_packer::pack::{self, PackOptions};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("roves-content-packer-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, contents: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn build_fake_dist(dist: &Path) {
    write(
        &dist.join("index.html"),
        br#"<html><head>
            <script type="module" src="./assets/entry.js"></script>
            <link rel="modulepreload" href="./assets/vendor.js">
        </head><body></body></html>"#,
    );
    write(&dist.join("assets/entry.js"), b"console.log('entry')");
    write(&dist.join("assets/vendor.js"), b"console.log('vendor')");
    write(&dist.join("assets/lazy-level1.js"), b"console.log('level 1, loaded on demand')");
    write(&dist.join("images/hero.png"), &[0u8; 128]);
    write(&dist.join("images/levels/level1-bg.png"), &[1u8; 256]);
    write(&dist.join("audio/theme.wav"), &[2u8; 64]);
}

fn pack_it(dist: &Path, out: &Path) {
    pack::pack(&PackOptions {
        input: dist.to_path_buf(),
        output: out.to_path_buf(),
        level: 1,
        max_pack_size: 500_000_000,
        exclude: vec![],
        html_file: "index.html".to_string(),
        boot_include: vec![],
    })
    .expect("pack should succeed");
}

#[test]
fn boot_set_is_html_plus_directly_referenced_files_only() {
    let root = scratch("boot-set");
    let dist = root.join("dist");
    let out = root.join("packed");
    build_fake_dist(&dist);
    pack_it(&dist, &out);

    let manifest = extract::load_manifest(&out).unwrap();
    let boot_pack_names: Vec<&str> = manifest.packs.iter().filter(|p| p.boot).map(|p| p.file.as_str()).collect();
    assert!(!boot_pack_names.is_empty(), "expected at least one boot pack");

    let pack_for = |rel: &str| manifest.files.get(rel).map(|s| s.as_str());
    let is_boot_pack = |name: Option<&str>| name.is_some_and(|n| boot_pack_names.contains(&n));

    assert!(is_boot_pack(pack_for("index.html")), "the html file itself must always be in the boot set");
    assert!(is_boot_pack(pack_for("assets/entry.js")), "the directly-<script src>'d entry chunk must be in the boot set");
    assert!(is_boot_pack(pack_for("assets/vendor.js")), "a modulepreload-referenced chunk must be in the boot set");
    assert!(
        !is_boot_pack(pack_for("assets/lazy-level1.js")),
        "a chunk never referenced by index.html must NOT be in the boot set"
    );
    assert!(
        !is_boot_pack(pack_for("images/levels/level1-bg.png")),
        "an image never referenced by index.html must NOT be in the boot set"
    );
}

#[test]
fn boot_extraction_only_materializes_boot_files() {
    let root = scratch("boot-extraction");
    let dist = root.join("dist");
    let out = root.join("packed");
    let dest = root.join("cache");
    build_fake_dist(&dist);
    pack_it(&dist, &out);

    let extracted = extract::extract_boot(&ExtractOptions {
        content_dir: out.clone(),
        dest: Some(dest.clone()),
        force: false,
    })
    .expect("extract_boot should succeed");
    assert_eq!(extracted, dest);

    assert!(dest.join("assets/entry.js").exists(), "directly-referenced script should be extracted eagerly");
    assert!(dest.join("assets/vendor.js").exists(), "modulepreload-referenced script should be extracted eagerly");
    assert!(
        !dest.join("assets/lazy-level1.js").exists(),
        "a file never referenced by index.html should NOT be extracted eagerly"
    );
    assert!(!dest.join("images/levels/level1-bg.png").exists(), "unreferenced image should stay lazy");
}

#[test]
fn on_demand_extraction_materializes_exactly_the_requested_files_pack() {
    let root = scratch("on-demand");
    let dist = root.join("dist");
    let out = root.join("packed");
    let dest = root.join("cache");
    build_fake_dist(&dist);
    pack_it(&dist, &out);

    extract::extract_boot(&ExtractOptions { content_dir: out.clone(), dest: Some(dest.clone()), force: false })
        .unwrap();
    assert!(!dest.join("assets/lazy-level1.js").exists());

    let manifest = extract::load_manifest(&out).unwrap();
    let found = extract::ensure_file_available(&out, &dest, &manifest, "assets/lazy-level1.js").unwrap();
    assert!(found, "lazy-level1.js is a real packed file, lookup must succeed");
    assert!(dest.join("assets/lazy-level1.js").exists(), "the requested file must now be on disk");

    // A file from a *different* still-untouched bucket must remain lazy —
    // on-demand extraction is per-pack, not "extract everything once anything is touched".
    assert!(
        !dest.join("images/levels/level1-bg.png").exists(),
        "requesting one lazy file must not eagerly extract unrelated lazy packs"
    );

    let not_packed = extract::ensure_file_available(&out, &dest, &manifest, "does/not/exist.js").unwrap();
    assert!(!not_packed, "a path with no manifest entry must report false, not error");
}

#[test]
fn unchanged_content_keeps_previously_extracted_lazy_files_across_relaunches() {
    let root = scratch("cache-reuse");
    let dist = root.join("dist");
    let out = root.join("packed");
    let dest = root.join("cache");
    build_fake_dist(&dist);
    pack_it(&dist, &out);

    extract::extract_boot(&ExtractOptions { content_dir: out.clone(), dest: Some(dest.clone()), force: false })
        .unwrap();
    let manifest = extract::load_manifest(&out).unwrap();
    extract::ensure_file_available(&out, &dest, &manifest, "assets/lazy-level1.js").unwrap();
    assert!(dest.join("assets/lazy-level1.js").exists());

    // Simulate a second launch: extract_boot runs again, content is unchanged.
    extract::extract_boot(&ExtractOptions { content_dir: out.clone(), dest: Some(dest.clone()), force: false })
        .unwrap();
    assert!(
        dest.join("assets/lazy-level1.js").exists(),
        "a lazily-extracted file from a previous session must survive a relaunch with unchanged content"
    );
}

#[test]
fn changed_content_wipes_the_destination_including_stale_lazy_extractions() {
    let root = scratch("content-changed");
    let dist = root.join("dist");
    let out = root.join("packed");
    let dest = root.join("cache");
    build_fake_dist(&dist);
    pack_it(&dist, &out);

    extract::extract_boot(&ExtractOptions { content_dir: out.clone(), dest: Some(dest.clone()), force: false })
        .unwrap();
    let manifest = extract::load_manifest(&out).unwrap();
    extract::ensure_file_available(&out, &dest, &manifest, "assets/lazy-level1.js").unwrap();
    assert!(dest.join("assets/lazy-level1.js").exists());

    // Change the source content and re-pack into the same `out` location.
    write(&dist.join("assets/lazy-level1.js"), b"console.log('level 1, changed content')");
    pack_it(&dist, &out);

    extract::extract_boot(&ExtractOptions { content_dir: out.clone(), dest: Some(dest.clone()), force: false })
        .unwrap();
    assert!(
        !dest.join("assets/lazy-level1.js").exists(),
        "changed content must wipe stale lazily-extracted files, not silently keep old ones around"
    );
}

#[test]
fn excluded_files_are_always_present_after_boot_extraction_and_never_in_the_files_map() {
    let root = scratch("excluded");
    let dist = root.join("dist");
    let out = root.join("packed");
    let dest = root.join("cache");
    build_fake_dist(&dist);
    write(&dist.join("save/profile.json"), b"{}");

    pack::pack(&PackOptions {
        input: dist.clone(),
        output: out.clone(),
        level: 1,
        max_pack_size: 500_000_000,
        exclude: vec![glob::Pattern::new("save/**").unwrap()],
        html_file: "index.html".to_string(),
        boot_include: vec![],
    })
    .unwrap();

    let manifest = extract::load_manifest(&out).unwrap();
    assert!(manifest.files.get("save/profile.json").is_none(), "excluded files must not be in the packed-files map");
    assert!(manifest.excluded.contains(&"save/profile.json".to_string()));

    extract::extract_boot(&ExtractOptions { content_dir: out, dest: Some(dest.clone()), force: false }).unwrap();
    assert!(dest.join("save/profile.json").exists(), "excluded files must be copied during boot extraction");
}
