use tervia::locale::Language;
use tervia::preferences::Preferences;

#[test]
fn system_language_is_the_default_and_region_variants_resolve() {
    assert_eq!(Preferences::default().language, Language::System);
    for (code, expected) in [
        ("zh-Hans-CN", Language::Chinese),
        ("zh-Hans-TW", Language::Chinese),
        ("zh_TW", Language::TraditionalChinese),
        ("zh-Hant-HK", Language::TraditionalChinese),
        ("en-GB", Language::English),
        ("ja-JP", Language::Japanese),
        ("ko-KR", Language::Korean),
        ("fr-CA", Language::French),
        ("de-DE", Language::German),
        ("es-MX", Language::Spanish),
        ("pt-BR", Language::Portuguese),
        ("ru-RU", Language::Russian),
    ] {
        assert_eq!(Language::from_locale(code), Some(expected));
    }
    assert_eq!(
        Language::preferred(["xx-XX", "ja-JP", "en-US"]),
        Language::Japanese
    );
    assert_eq!(Language::preferred(["xx-XX"]), Language::English);
}

#[test]
fn catalogs_cover_each_other_and_preserve_interpolation_tokens() {
    Language::validate_catalogs().unwrap();
    assert_eq!(Language::English.text("settings"), "Settings");
    assert_eq!(Language::Japanese.text("settings"), "設定");
    assert_eq!(
        Language::French.text("unknown_synthetic_label"),
        "unknown_synthetic_label"
    );
    assert_eq!(
        Language::English.format("view_diff", &[("path", "mock/{count}.rs")]),
        "View diff for mock/{count}.rs",
    );
}
