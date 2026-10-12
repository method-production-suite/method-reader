//! Port of tests/route-facts.test.ts: the probes behind the route catalog,
//! every one injected, so nothing here touches the real machine. The two
//! checks of the DEFAULT device probe (listDevices with its options) wait
//! for device detection (the sp-device branch): here the caller always
//! hands the device list in.

mod common;

use std::cell::{Cell, RefCell};

use common::env_of;
use sp_engine::device_types::{ConnectedDevice, DeviceKind};
use sp_engine::env::Env;
use sp_engine::export::route_facts::{
    CommandResult, MailtoHandlerResult, RouteProbes, parse_mailto_handler, route_facts,
};
use sp_engine::export::routes::RouteFacts;

fn kindle() -> ConnectedDevice {
    ConnectedDevice {
        kind: DeviceKind::Kindle,
        name: "Kindle".into(),
        volume: Some("/Volumes/Kindle".into()),
    }
}

/// Every probe stubbed to something harmless and darwin-flavoured.
fn base<'a>() -> RouteProbes<'a> {
    RouteProbes {
        platform: Some("darwin".into()),
        devices: Some(Box::new(Vec::new)),
        exists: Some(Box::new(|_| false)),
        mailto_handler: Some(Box::new(|| Some(None))),
        env: Some(Env::new()),
        run: None,
    }
}

fn facts(probes: &RouteProbes) -> RouteFacts {
    route_facts(probes, false)
}

// ---- parseMailtoHandler ----

#[test]
fn the_exact_shape_measured_on_the_owners_mac() {
    let output = r#"(
    {
        LSHandlerContentType = "public.svg-image";
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.google.Chrome";
    },
    {
        LSHandlerModificationDate = 727881812;
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.superhuman.electron";
        LSHandlerURLScheme = mailto;
    },
    {
        LSHandlerContentType = "public.plain-text";
        LSHandlerRoleAll = "com.apple.TextEdit";
    }
)"#;
    assert_eq!(
        parse_mailto_handler(output).as_deref(),
        Some("com.superhuman.electron")
    );
}

#[test]
fn a_bare_value_on_both_sides() {
    let output = "(\n    {\n        LSHandlerRoleAll = com.apple.mail;\n        LSHandlerURLScheme = mailto;\n    }\n)";
    assert_eq!(parse_mailto_handler(output).as_deref(), Some("com.apple.mail"));
}

#[test]
fn no_mailto_dict_reads_as_none() {
    let output = r#"(
    {
        LSHandlerContentType = "public.plain-text";
        LSHandlerRoleAll = "com.apple.TextEdit";
    },
    {
        LSHandlerContentType = "public.svg-image";
        LSHandlerRoleAll = "com.google.Chrome";
    }
)"#;
    assert_eq!(parse_mailto_handler(output), None);
}

#[test]
fn garbage_reads_as_none() {
    assert_eq!(parse_mailto_handler("not a plist at all {{{ broken )"), None);
    assert_eq!(parse_mailto_handler(""), None);
}

// ---- booksApp ----

#[test]
fn books_true_at_the_system_applications_path() {
    let mut p = base();
    p.exists = Some(Box::new(|p| p == "/System/Applications/Books.app"));
    assert!(facts(&p).books_app);
}

#[test]
fn books_true_at_the_applications_path() {
    let mut p = base();
    p.exists = Some(Box::new(|p| p == "/Applications/Books.app"));
    assert!(facts(&p).books_app);
}

#[test]
fn books_false_at_neither() {
    assert!(!facts(&base()).books_app);
}

// ---- sendToKindleApp ----

#[test]
fn send_to_kindle_true_when_it_exists() {
    let mut p = base();
    p.exists = Some(Box::new(|p| p == "/Applications/Send to Kindle.app"));
    assert!(facts(&p).send_to_kindle_app);
}

#[test]
fn send_to_kindle_true_in_the_home_folders_applications() {
    let asked = RefCell::new(Vec::<String>::new());
    let mut p = base();
    p.env = Some(env_of(&[("HOME", "/Users/tester")]));
    p.exists = Some(Box::new(|path| {
        asked.borrow_mut().push(path.to_owned());
        path == "/Users/tester/Applications/Send to Kindle.app"
    }));
    assert!(facts(&p).send_to_kindle_app);
    drop(p);
    assert!(asked.borrow().contains(&"/Applications/Send to Kindle.app".to_owned()));
}

#[test]
fn the_home_folder_comes_from_home_then_userprofile_then_the_os() {
    let found = |env: Env, home: String| {
        let want = format!("{home}/Applications/Send to Kindle.app");
        let mut p = base();
        p.env = Some(env);
        p.exists = Some(Box::new(move |path| path == want));
        facts(&p).send_to_kindle_app
    };
    assert!(found(
        env_of(&[("HOME", "/Users/a"), ("USERPROFILE", "/Users/b")]),
        "/Users/a".into()
    ));
    assert!(found(env_of(&[("USERPROFILE", "/Users/b")]), "/Users/b".into()));
    assert!(found(Env::new(), sp_engine::env::os_home()));
    assert!(!found(env_of(&[("HOME", "/Users/a")]), "/Users/b".into()));
}

#[test]
fn send_to_kindle_false_when_it_does_not_exist() {
    assert!(!facts(&base()).send_to_kindle_app);
}

// ---- appleMailDefault ----

fn mail(answer: MailtoHandlerResult) -> bool {
    let mut p = base();
    p.mailto_handler = Some(Box::new(move || answer.clone()));
    facts(&p).apple_mail_default
}

#[test]
fn apple_mail_when_there_is_no_mailto_entry() {
    assert!(mail(Some(None)));
}

#[test]
fn apple_mail_for_com_apple_mail_any_case() {
    assert!(mail(Some(Some("com.apple.mail".into()))));
    assert!(mail(Some(Some("COM.APPLE.MAIL".into()))));
}

#[test]
fn not_apple_mail_for_a_third_party_handler() {
    assert!(!mail(Some(Some("com.superhuman.electron".into()))));
}

#[test]
fn not_apple_mail_when_the_handler_could_not_be_read() {
    assert!(!mail(None));
}

// ---- the real mail probe, through its runner ----

const ABSENT: &str = "2026-09-23 18:07:32.137 defaults[94708:26435202] \nThe domain/default pair of (com.apple.LaunchServices/com.apple.launchservices.secure, LSHandlers) does not exist\n";
const SUPERHUMAN: &str = "(\n    {\n        LSHandlerPreferredVersions =         {\n            LSHandlerRoleAll = \"-\";\n        };\n        LSHandlerRoleAll = \"com.superhuman.electron\";\n        LSHandlerURLScheme = mailto;\n    }\n)";
const NO_MAILTO: &str = "(\n    {\n        LSHandlerContentType = \"public.plain-text\";\n        LSHandlerRoleAll = \"com.apple.TextEdit\";\n    }\n)";

fn result(code: i32, stdout: &str, stderr: &str) -> Result<CommandResult, String> {
    Ok(CommandResult {
        code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    })
}

/// The default mail probe, run through a runner that records every argv.
fn via_runner(
    platform: &str,
    answer: Result<CommandResult, String>,
) -> (RouteFacts, Vec<Vec<String>>) {
    let argvs = RefCell::new(Vec::new());
    let facts = {
        let mut p = base();
        p.platform = Some(platform.into());
        p.mailto_handler = None;
        p.run = Some(Box::new(|argv: &[&str]| {
            argvs
                .borrow_mut()
                .push(argv.iter().map(|s| (*s).to_owned()).collect());
            answer.clone()
        }));
        facts(&p)
    };
    (facts, argvs.into_inner())
}

#[test]
fn asks_launch_services_exactly_once() {
    let (_, argvs) = via_runner("darwin", result(0, NO_MAILTO, ""));
    assert_eq!(
        argvs,
        vec![vec![
            "defaults",
            "read",
            "com.apple.LaunchServices/com.apple.launchservices.secure",
            "LSHandlers"
        ]]
    );
}

#[test]
fn exit_1_saying_does_not_exist_is_apple_mail() {
    assert!(via_runner("darwin", result(1, "", ABSENT)).0.apple_mail_default);
    assert!(via_runner("darwin", result(1, ABSENT, "")).0.apple_mail_default);
}

#[test]
fn any_other_failure_could_not_tell() {
    for stderr in ["", "Could not open the preferences database\n", "permission denied\n"] {
        assert!(
            !via_runner("darwin", result(1, "", stderr)).0.apple_mail_default,
            "{stderr:?}"
        );
    }
}

#[test]
fn a_runner_that_fails_to_start_could_not_tell() {
    assert!(
        !via_runner("darwin", Err("spawn defaults ENOENT".into()))
            .0
            .apple_mail_default
    );
}

#[test]
fn exit_0_naming_another_mail_app_is_not_apple_mail() {
    assert!(!via_runner("darwin", result(0, SUPERHUMAN, "")).0.apple_mail_default);
}

#[test]
fn exit_0_with_no_mailto_entry_is_apple_mail() {
    assert!(via_runner("darwin", result(0, NO_MAILTO, "")).0.apple_mail_default);
}

#[test]
fn exit_0_is_read_from_stdout_whatever_stderr_says() {
    assert!(!via_runner("darwin", result(0, SUPERHUMAN, ABSENT)).0.apple_mail_default);
}

#[test]
fn off_darwin_the_runner_is_never_called() {
    for platform in ["win32", "linux"] {
        assert!(via_runner(platform, result(1, "", ABSENT)).1.is_empty());
    }
}

// ---- off darwin ----

#[test]
fn off_darwin_the_mac_facts_are_false_however_the_probes_answer() {
    for platform in ["win32", "linux"] {
        let mut p = base();
        p.platform = Some(platform.into());
        p.exists = Some(Box::new(|_| true));
        p.mailto_handler = Some(Box::new(|| Some(Some("com.apple.mail".into()))));
        let f = facts(&p);
        assert!(!f.books_app && !f.send_to_kindle_app && !f.apple_mail_default);
    }
}

#[test]
fn off_darwin_exists_and_mailto_are_never_called() {
    for platform in ["win32", "linux"] {
        let exists_calls = Cell::new(0);
        let mailto_calls = Cell::new(0);
        {
            let mut p = base();
            p.platform = Some(platform.into());
            p.exists = Some(Box::new(|_| {
                exists_calls.set(exists_calls.get() + 1);
                true
            }));
            p.mailto_handler = Some(Box::new(|| {
                mailto_calls.set(mailto_calls.get() + 1);
                Some(None)
            }));
            facts(&p);
        }
        assert_eq!((exists_calls.get(), mailto_calls.get()), (0, 0));
    }
}

// ---- devices ----

#[test]
fn the_devices_probe_always_runs_and_is_carried_through() {
    for platform in ["darwin", "win32", "linux"] {
        let called = Cell::new(false);
        let f = {
            let mut p = base();
            p.platform = Some(platform.into());
            p.devices = Some(Box::new(|| {
                called.set(true);
                vec![kindle()]
            }));
            facts(&p)
        };
        assert!(called.get());
        assert_eq!(f.devices, vec![kindle()]);
    }
}

// ---- platform ----

#[test]
fn the_platform_defaults_to_this_one() {
    let mut p = base();
    p.platform = None;
    assert_eq!(facts(&p).platform, sp_engine::env::host_platform());
}

// ---- the quick answer ----

#[test]
fn skip_remarkable_says_the_tablet_was_not_looked_for() {
    let mut p = base();
    p.devices = Some(Box::new(|| vec![kindle()]));
    let f = route_facts(&p, true);
    assert_eq!(f.devices, vec![kindle()]);
    assert_eq!(f.remarkable_checked, Some(false));
}

#[test]
fn a_full_answer_carries_no_such_fact() {
    let mut p = base();
    p.devices = Some(Box::new(|| vec![kindle()]));
    let f = route_facts(&p, false);
    assert_eq!(f.remarkable_checked, None);
}
