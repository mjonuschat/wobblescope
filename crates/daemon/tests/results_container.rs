mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
use tempfile::TempDir;
use wobblescope::results::container::{ResultError, ResultFile, write_result};
use wobblescope::results::record::{AxisSnapshot, RESULT_VERSION, ResultRecord, Snippets};
use wobblescope_core::eligibility::Eligibility;
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::Analysis;
use wobblescope_session::codec::encode_samples;
use wobblescope_session::meta::SessionMeta;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use common::{Spec, samples};

fn analysis() -> Analysis {
    Analysis {
        per_chip: vec![],
        worst_case: None,
        worst_case_vibrations: vec![],
        summed: None,
        failures: vec![],
        eligibility: Eligibility { decisions: vec![] },
        core_version: "0.1.0".to_owned(),
    }
}

fn record(manifest: BTreeMap<String, String>) -> ResultRecord {
    let axis = |axis| AxisSnapshot {
        axis,
        analysis: analysis(),
        snippets: Snippets {
            worst_case: Some("[input_shaper]".to_owned()),
            per_chip: vec![None, Some(vec!["[input_shaper]".to_owned()])],
        },
    };
    ResultRecord {
        version: RESULT_VERSION,
        id: "res-1".to_owned(),
        created_at: "2026-11-10T10:00:00Z".to_owned(),
        label: Some("before the belt change".to_owned()),
        session_id: "s1".to_owned(),
        core_version: "0.1.0".to_owned(),
        daemon_version: "0.1.0".to_owned(),
        fit_params: FitParams::default(),
        overrides: vec![],
        axes: vec![axis(Axis::X), axis(Axis::Y)],
        manifest,
    }
}

fn meta() -> SessionMeta {
    Spec::new("s1", "2026-11-10T09:00:00Z")
        .chips(&["a", "b"])
        .samples(200)
        .meta()
}

fn meta_bytes() -> Vec<u8> {
    meta().to_json().unwrap().into_bytes()
}

fn sample_bytes(seed: f64) -> Vec<u8> {
    encode_samples(&samples(200, seed)).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn manifest_of(entries: &[(&str, Vec<u8>)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .filter(|(name, _)| *name != "result.json")
        .map(|(name, bytes)| ((*name).to_owned(), hex(bytes)))
        .collect()
}

fn good_entries() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("meta.json", meta_bytes()),
        ("samples/c0-s0.zst", sample_bytes(0.0)),
        ("samples/c1-s0.zst", sample_bytes(1.0)),
    ]
}

fn hostile(dir: &Path, entries: &[(&str, Vec<u8>)]) -> PathBuf {
    let json = serde_json::to_vec(&record(manifest_of(entries))).unwrap();
    hostile_with_record(dir, entries, json)
}

fn hostile_with_record(dir: &Path, entries: &[(&str, Vec<u8>)], record_json: Vec<u8>) -> PathBuf {
    let path = dir.join("hostile.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::DEFAULT);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("result.json", options).unwrap();
    zip.write_all(&record_json).unwrap();
    zip.finish().unwrap();
    path
}

fn replace_all(path: &Path, from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut bytes = fs::read(path).unwrap();
    let mut found = 0;
    for i in 0..bytes.len().saturating_sub(from.len()) {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            found += 1;
        }
    }
    assert!(found > 0, "pattern not found");
    fs::write(path, bytes).unwrap();
}

fn is_format(r: Result<ResultFile, ResultError>) -> bool {
    matches!(r, Err(ResultError::Format(_)))
}

fn verify_error(entries: Vec<(&'static str, Vec<u8>)>) -> ResultError {
    let dir = TempDir::new().unwrap();
    let path = hostile(dir.path(), &entries);
    ResultFile::open(&path).unwrap().verify().unwrap_err()
}

#[test]
fn writes_and_reads_a_result() {
    let dir = TempDir::new().unwrap();
    let (s0, s1) = (sample_bytes(0.0), sample_bytes(1.0));
    fs::write(dir.path().join("a.zst"), &s0).unwrap();
    fs::write(dir.path().join("b.zst"), &s1).unwrap();
    let dest = dir.path().join("res-1.zip");

    write_result(
        &dest,
        &record(BTreeMap::new()),
        &meta_bytes(),
        &[
            ("c0-s0.zst".to_owned(), dir.path().join("a.zst")),
            ("c1-s0.zst".to_owned(), dir.path().join("b.zst")),
        ],
    )
    .unwrap();

    let file = ResultFile::open(&dest).unwrap();
    assert_eq!(file.record().id, "res-1");
    file.verify().unwrap();
    assert_eq!(file.meta().unwrap(), meta());
    assert_eq!(file.read_sample("c0-s0.zst").unwrap(), s0);
    assert_eq!(file.read_sample("c1-s0.zst").unwrap(), s1);
    let keys: Vec<&str> = file.record().manifest.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["meta.json", "samples/c0-s0.zst", "samples/c1-s0.zst"]
    );
}

#[test]
fn refuses_a_compressed_entry() {
    let dir = TempDir::new().unwrap();
    let path = hostile(dir.path(), &good_entries());
    let mut bytes = fs::read(&path).unwrap();
    for i in 0..bytes.len() - 12 {
        if bytes[i..i + 4] == *b"PK\x03\x04" {
            bytes[i + 8] = 8;
        } else if bytes[i..i + 4] == *b"PK\x01\x02" {
            bytes[i + 10] = 8;
        }
    }
    fs::write(&path, bytes).unwrap();
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_a_parent_directory_entry() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    entries.push(("samples/..x", vec![1]));
    let path = hostile(dir.path(), &entries);
    replace_all(&path, b"samples/..x", b"samples/../");
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_a_duplicate_entry() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    entries.push(("meta.jsox", meta_bytes()));
    let path = hostile(dir.path(), &entries);
    replace_all(&path, b"meta.jsox", b"meta.json");
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_an_unlisted_entry() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    let json = serde_json::to_vec(&record(manifest_of(&entries))).unwrap();
    entries.push(("samples/c2-s0.zst", sample_bytes(2.0)));
    let path = hostile_with_record(dir.path(), &entries, json);
    let file = ResultFile::open(&path).unwrap();
    assert!(matches!(file.verify(), Err(ResultError::Corrupt(_))));
}

#[test]
fn refuses_a_missing_manifest_entry() {
    let dir = TempDir::new().unwrap();
    let entries = good_entries();
    let mut manifest = manifest_of(&entries);
    manifest.insert("samples/c2-s0.zst".to_owned(), hex(b"absent"));
    let json = serde_json::to_vec(&record(manifest)).unwrap();
    let path = hostile_with_record(dir.path(), &entries, json);
    let file = ResultFile::open(&path).unwrap();
    assert!(matches!(file.verify(), Err(ResultError::Corrupt(_))));
}

#[test]
fn refuses_a_newer_version() {
    let dir = TempDir::new().unwrap();
    let entries = good_entries();
    let mut value = serde_json::to_value(record(manifest_of(&entries))).unwrap();
    value["version"] = (RESULT_VERSION + 1).into();
    let path = hostile_with_record(dir.path(), &entries, serde_json::to_vec(&value).unwrap());
    assert!(matches!(
        ResultFile::open(&path),
        Err(ResultError::TooNew(2))
    ));
}

#[test]
fn detects_a_flipped_sample_byte() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    let json = serde_json::to_vec(&record(manifest_of(&entries))).unwrap();
    entries[1].1[30] ^= 0xFF;
    let path = hostile_with_record(dir.path(), &entries, json);
    let file = ResultFile::open(&path).unwrap();
    match file.verify() {
        Err(ResultError::Corrupt(message)) => assert!(message.contains("samples/c0-s0.zst")),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

fn frame(n: u64, checksum: bool) -> Vec<u8> {
    let mut payload = b"WSSAMPLE".to_vec();
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&n.to_le_bytes());
    payload.resize(payload.len() + 32 * n as usize, 0);
    let mut compressor = FrameCompressor::new(CompressionLevel::Default);
    compressor.set_content_checksum(checksum);
    compressor.set_source(payload.as_slice());
    compressor.set_drain(Vec::new());
    compressor.compress();
    compressor.take_drain().unwrap()
}

fn with_first_sample(bytes: Vec<u8>) -> Vec<(&'static str, Vec<u8>)> {
    let mut entries = good_entries();
    entries[1].1 = bytes;
    entries
}

#[test]
fn refuses_a_frame_without_a_checksum() {
    let error = verify_error(with_first_sample(frame(200, false)));
    assert!(matches!(error, ResultError::Corrupt(_)), "{error:?}");
}

#[test]
fn refuses_a_frame_with_an_extra_frame() {
    let mut bytes = frame(200, true);
    bytes.extend(frame(200, true));
    let error = verify_error(with_first_sample(bytes));
    assert!(matches!(error, ResultError::Corrupt(_)), "{error:?}");
}

#[test]
fn refuses_a_frame_over_the_window_limit() {
    // Magic, descriptor with a content checksum, a 32 MiB window, one empty raw last block.
    let mut bytes = vec![0x28, 0xB5, 0x2F, 0xFD, 0x04, 0x78, 0x01, 0x00, 0x00];
    bytes.extend_from_slice(&[0; 4]);
    let error = verify_error(with_first_sample(bytes));
    assert!(matches!(error, ResultError::Corrupt(_)), "{error:?}");
}

#[test]
fn refuses_a_sample_count_that_differs_from_the_metadata() {
    let error = verify_error(with_first_sample(frame(199, true)));
    assert!(matches!(error, ResultError::Corrupt(_)), "{error:?}");
}

#[test]
fn refuses_a_symlink_entry() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("link.zip");
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::DEFAULT);
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.add_symlink("samples/c0-s0.zst", "/etc/passwd", options)
        .unwrap();
    zip.start_file("result.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&record(BTreeMap::new())).unwrap())
        .unwrap();
    zip.finish().unwrap();
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_zip64() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("big.zip");
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::DEFAULT)
        .large_file(true);
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file("meta.json", options).unwrap();
    zip.write_all(&meta_bytes()).unwrap();
    zip.start_file("result.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&record(BTreeMap::new())).unwrap())
        .unwrap();
    zip.finish().unwrap();
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_an_absolute_path() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    entries.push(("xsamples/c2-s0.zst", vec![1]));
    let path = hostile(dir.path(), &entries);
    replace_all(&path, b"xsamples/c2-s0.zst", b"/samples/c2-s0.zst");
    assert!(is_format(ResultFile::open(&path)));
}

#[test]
fn refuses_a_name_outside_the_allowlist() {
    let dir = TempDir::new().unwrap();
    let mut entries = good_entries();
    entries.push(("notes.txt", vec![1]));
    let path = hostile(dir.path(), &entries);
    assert!(is_format(ResultFile::open(&path)));
}
