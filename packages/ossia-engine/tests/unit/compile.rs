use super::*;

#[test]
fn output_groups_require_contiguous_numeric_pages_and_unique_exports() {
    let dir = tempfile::tempdir().unwrap();
    for n in [10, 3, 1, 7, 4, 8, 2, 5, 9, 6] {
        fs::write(dir.path().join(format!("score-{n}.svg")), "<svg/>").unwrap();
    }
    assert_eq!(
        collect(dir.path(), RenderFormat::Svg)
            .unwrap()
            .iter()
            .map(|x| x.0)
            .collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    fs::remove_file(dir.path().join("score-5.svg")).unwrap();
    assert!(collect(dir.path(), RenderFormat::Svg).is_err());
    fs::write(dir.path().join("score.pdf"), "%PDF-1.4").unwrap();
    fs::write(dir.path().join("score-other.pdf"), "%PDF-1.4").unwrap();
    assert!(collect(dir.path(), RenderFormat::Pdf).is_err());
    fs::write(dir.path().join("another.svg"), "<svg/>").unwrap();
    assert!(collect(dir.path(), RenderFormat::Svg).is_err());
}
