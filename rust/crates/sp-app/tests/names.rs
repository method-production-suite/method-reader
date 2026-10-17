//! The product's name in the window, and the two environment variables a
//! person might have set under the old name: METHOD_READER_CONFIG_DIR and
//! METHOD_READER_LIBRARY are read first, SCREEPUB_CONFIG_DIR and
//! SCREEPUB_LIBRARY only when the new one is unset.

use sp_app::model::names::{
    CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR, LEGACY_LIBRARY_VAR, LIBRARY_VAR, PRODUCT,
    var_with_legacy,
};

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn the_names() {
    assert_eq!(PRODUCT, "Method Reader");
    assert_eq!(sp_app::model::convert::WORDMARK, "Method Reader");
    assert_eq!(CONFIG_DIR_VAR, "METHOD_READER_CONFIG_DIR");
    assert_eq!(LEGACY_CONFIG_DIR_VAR, "SCREEPUB_CONFIG_DIR");
    assert_eq!(LIBRARY_VAR, "METHOD_READER_LIBRARY");
    assert_eq!(LEGACY_LIBRARY_VAR, "SCREEPUB_LIBRARY");
}

#[test]
fn the_new_name_wins_and_the_old_one_is_the_fallback() {
    let both = env(&[
        ("METHOD_READER_LIBRARY", "/new"),
        ("SCREEPUB_LIBRARY", "/old"),
    ]);
    assert_eq!(
        var_with_legacy(&both, LIBRARY_VAR, LEGACY_LIBRARY_VAR),
        Some(("/new".into(), LIBRARY_VAR))
    );
    let old = env(&[("SCREEPUB_LIBRARY", "/old")]);
    assert_eq!(
        var_with_legacy(&old, LIBRARY_VAR, LEGACY_LIBRARY_VAR),
        Some(("/old".into(), LEGACY_LIBRARY_VAR))
    );
    let none = env(&[]);
    assert_eq!(
        var_with_legacy(&none, LIBRARY_VAR, LEGACY_LIBRARY_VAR),
        None
    );
}

#[test]
fn an_empty_value_counts_as_unset() {
    let empty_new = env(&[
        ("METHOD_READER_CONFIG_DIR", ""),
        ("SCREEPUB_CONFIG_DIR", "/old"),
    ]);
    assert_eq!(
        var_with_legacy(&empty_new, CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR),
        Some(("/old".into(), LEGACY_CONFIG_DIR_VAR))
    );
    let empty_both = env(&[
        ("METHOD_READER_CONFIG_DIR", ""),
        ("SCREEPUB_CONFIG_DIR", ""),
    ]);
    assert_eq!(
        var_with_legacy(&empty_both, CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR),
        None
    );
}

#[test]
fn the_library_line_names_the_variable_that_set_it() {
    use sp_app::model::convert::{library_from, library_line_for};
    let answer = serde_json::json!({"ok": true, "home": "/Users/ann", "library": {
        "path": "/srv/books", "chosen": null, "platformDefault": "/d", "fromEnv": true}});
    let l = library_from(&answer).unwrap();
    assert_eq!(
        library_line_for(&l, LIBRARY_VAR),
        "Books are saved in /srv/books, set by METHOD_READER_LIBRARY."
    );
    assert_eq!(
        library_line_for(&l, LEGACY_LIBRARY_VAR),
        "Books are saved in /srv/books, set by SCREEPUB_LIBRARY."
    );
}
