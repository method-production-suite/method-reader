//! .github/dependabot.yml, read as the YAML Dependabot reads
//! (SUITE-STANDARDS.md section 6): one grouped pull request a month for the
//! Rust workspace, held to the same 14-day cooldown as CI's crate-age
//! check, security updates left separate, and no GitHub Actions entry,
//! since MOrg moves the suite's pinned action SHAs for every repository at
//! once.

mod repo_common;

use repo_common::*;
use serde_yaml::Value;

fn updates() -> Vec<Value> {
    let v: Value = serde_yaml::from_str(&read(".github/dependabot.yml")).expect("dependabot.yml");
    assert_eq!(v["version"].as_u64(), Some(2));
    v["updates"]
        .as_sequence()
        .expect("an updates: list")
        .to_vec()
}

#[test]
fn only_the_rust_workspace_is_updated() {
    let ecosystems: Vec<(String, String)> = updates()
        .iter()
        .map(|u| {
            (
                s(u, "package-ecosystem").to_string(),
                s(u, "directory").to_string(),
            )
        })
        .collect();
    assert_eq!(ecosystems, [("cargo".to_string(), "/rust".to_string())]);
    assert!(
        !read(".github/dependabot.yml").contains("package-ecosystem: github-actions"),
        "GitHub Actions stay out of Dependabot: MOrg moves the shared pins"
    );
}

#[test]
fn cargo_comes_monthly_as_one_group_after_14_days() {
    let all = updates();
    let cargo = &all[0];
    assert_eq!(cargo["schedule"]["interval"].as_str(), Some("monthly"));
    assert_eq!(cargo["cooldown"]["default-days"].as_u64(), Some(14));
    assert!(cargo["cooldown"]["semver-major-days"].as_u64() >= Some(14));
    let groups = cargo["groups"].as_mapping().expect("a groups: table");
    assert_eq!(groups.len(), 1, "one group, so one pull request a month");
    let (_, group) = groups.iter().next().expect("the group");
    // Version updates only: a security update stays its own pull request.
    assert_eq!(group["applies-to"].as_str(), Some("version-updates"));
    let patterns: Vec<&str> = group["patterns"]
        .as_sequence()
        .expect("patterns")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(patterns, ["*"]);
    // Every kind of version update joins the group, majors included.
    assert!(group.get("update-types").is_none());
}
