//! 端到端集成测试:通过公开 API 驱动完整破解流程,断言破出已知密码 `92eo`。
//! 不依赖 target/release 二进制,不使用 shell。

use password_cracking::engine::CrackerEngine;
use password_cracking::generators::rules::Rule;
use password_cracking::generators::{GeneratorMode, create_source};
use std::io::Write;
use std::path::PathBuf;

/// 例子文件的已知密码。
const KNOWN_PASSWORD: &str = "92eo";

/// 定位 crate 根下的例子文件,避免依赖当前工作目录。
fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

/// 写一个临时词表文件,返回持有它的 NamedTempFile(离开作用域即删除)。
/// 词表包含若干干扰词 + 已知密码。
fn wordlist_with_password() -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("create temp wordlist");
    writeln!(f, "wrong1").unwrap();
    writeln!(f, "wrong2").unwrap();
    writeln!(f, "{KNOWN_PASSWORD}").unwrap();
    writeln!(f, "wrong3").unwrap();
    f.flush().unwrap();
    f
}

/// 用给定 GeneratorMode 对给定文件跑一次破解,返回找到的密码。
fn crack(file: &std::path::Path, mode: GeneratorMode) -> Option<String> {
    let source = create_source(mode).expect("create source");
    let engine = CrackerEngine::new(file, source, Some(2)).expect("build engine");
    let result = engine.crack(true).expect("crack runs");
    result.password
}

#[test]
fn dictionary_cracks_zip() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_pdf() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.pdf"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_docx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.docx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_xlsx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.xlsx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_pptx() {
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.pptx"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn brute_force_cracks_zip() {
    // "92eo" = 数字数字小写小写,落在 alnum 长度 1..=4 空间内。
    let mode = GeneratorMode::BruteForce {
        charset: password_cracking::utils::charsets::alphanumeric(),
        min_length: 1,
        max_length: 4,
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn hybrid_cracks_zip() {
    // 词表里放小写形式,靠原词直出命中(hybrid 总是包含原词)。
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "decoy").unwrap();
    writeln!(f, "{KNOWN_PASSWORD}").unwrap();
    f.flush().unwrap();
    let mode = GeneratorMode::Hybrid {
        dictionary_path: f.path().to_path_buf(),
        rules: vec![Rule::Capitalize, Rule::L33t],
    };
    assert_eq!(crack(&example("test.zip"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn wrong_wordlist_returns_none() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "nope1").unwrap();
    writeln!(f, "nope2").unwrap();
    f.flush().unwrap();
    let mode = GeneratorMode::Dictionary {
        path: f.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test.zip"), mode), None);
}

#[test]
fn dictionary_cracks_pdf_r6() {
    // test_r6.pdf 是 PDF 2.0 AES-256 R6(Algorithm 2.B 硬化哈希)。
    // 修复前当前实现只做单轮 SHA256(R5 算法),对 R6 会失败。
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test_r6.pdf"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_pdf_aes128() {
    // test_aes128.pdf is PDF 1.6 AES-128 (V=4 R=4). Regression test for
    // top-level /Length parsing (the /CF filter length is in bytes) and
    // signed /P permissions parsing.
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test_aes128.pdf"), mode).as_deref(), Some(KNOWN_PASSWORD));
}

#[test]
fn dictionary_cracks_pdf_xref_stream() {
    // test_xref.pdf has no classic `trailer`: its trailer entries live in a
    // cross-reference stream dictionary (PDF 1.5+). Regression test for the
    // xref-stream fallback parser.
    let wl = wordlist_with_password();
    let mode = GeneratorMode::Dictionary {
        path: wl.path().to_path_buf(),
    };
    assert_eq!(crack(&example("test_xref.pdf"), mode).as_deref(), Some(KNOWN_PASSWORD));
}
