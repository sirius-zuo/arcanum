use arcanum_ingestion::MimeDetector;
use std::io::Write;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

fn epub_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("META-INF/container.xml",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"<?xml?>").unwrap();
    z.finish().unwrap().into_inner()
}

fn bare_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("data.txt",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"hello").unwrap();
    z.finish().unwrap().into_inner()
}

fn office_zip() -> Vec<u8> {
    let mut z = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("[Content_Types].xml",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
    z.write_all(b"<?xml?>").unwrap();
    z.finish().unwrap().into_inner()
}

#[test]
fn test_pdf_magic_detected() {
    let pdf = b"%PDF-1.4 content";
    assert_eq!(MimeDetector::detect(pdf, None), "application/pdf");
}

#[test]
fn test_epub_disambiguated_from_zip() {
    assert_eq!(MimeDetector::detect(&epub_zip(), None), "application/epub+zip");
}

#[test]
fn test_office_zip_detected() {
    assert_eq!(MimeDetector::detect(&office_zip(), None), "application/vnd.openxmlformats");
}

#[test]
fn test_bare_zip_stays_zip() {
    assert_eq!(MimeDetector::detect(&bare_zip(), None), "application/zip");
}

#[test]
fn test_hint_used_when_no_magic() {
    assert_eq!(MimeDetector::detect(b"# markdown", Some("text/markdown")), "text/markdown");
}

#[test]
fn test_fallback_to_octet_stream() {
    assert_eq!(MimeDetector::detect(b"no magic here", None), "application/octet-stream");
}

#[test]
fn test_html_magic_detected() {
    assert_eq!(MimeDetector::detect(b"<!DOCTYPE html><html>", None), "text/html");
}
