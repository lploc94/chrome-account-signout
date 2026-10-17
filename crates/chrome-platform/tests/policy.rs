use chrome_platform::{Process, classify};
use std::{collections::HashSet, path::PathBuf};
fn p(id: u32, parent: u32, args: &[&str]) -> Process {
    Process {
        pid: id,
        parent,
        birth: id as u64,
        executable: PathBuf::from("/chrome"),
        args: Some(
            args.iter()
                .map(|s| {
                    if cfg!(windows) {
                        s.replace("=/", "=C:/")
                    } else {
                        s.to_string()
                    }
                })
                .collect(),
        ),
    }
}
fn norm(p: &std::path::Path) -> signout_core::Result<PathBuf> {
    if cfg!(windows) && p.to_string_lossy().starts_with('/') {
        Ok(PathBuf::from(format!("C:{}", p.display())))
    } else {
        Ok(p.to_path_buf())
    }
}
#[test]
fn windows_typed_children_inherit_automation_owner() {
    let rows = vec![
        p(1, 0, &["chrome", "--user-data-dir=/temp"]),
        p(2, 1, &["chrome", "--type=crashpad-handler"]),
        p(3, 1, &["chrome", "--type=renderer"]),
    ];
    let state = classify(
        &rows,
        &PathBuf::from("/chrome"),
        &PathBuf::from("/real"),
        &HashSet::new(),
        &norm,
    )
    .unwrap();
    assert!(state.main.is_empty());
    assert!(state.relevant.is_empty());
}
#[test]
fn primary_tree_and_orphan_tracking() {
    let rows = vec![
        p(1, 0, &["chrome"]),
        p(2, 1, &["chrome", "--type=renderer"]),
    ];
    let state = classify(
        &rows,
        &PathBuf::from("/chrome"),
        &PathBuf::from("/real"),
        &HashSet::new(),
        &norm,
    )
    .unwrap();
    assert_eq!(state.main.len(), 1);
    assert_eq!(state.relevant.len(), 2);
    let orphan = classify(
        &rows[1..],
        &PathBuf::from("/chrome"),
        &PathBuf::from("/real"),
        &state.relevant,
        &norm,
    )
    .unwrap();
    assert_eq!(orphan.relevant.len(), 1);
}
#[test]
fn unknown_child_and_reused_pid_fail() {
    let rows = vec![p(2, 1, &["chrome", "--type=renderer"])];
    assert!(
        classify(
            &rows,
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::from([(2, 99)]),
            &norm
        )
        .is_err()
    );
}
#[test]
fn duplicate_relative_and_unknown_args_refused() {
    for args in [
        vec!["chrome", "--user-data-dir=/a", "--user-data-dir=/b"],
        vec!["chrome", "--user-data-dir=relative"],
        vec!["chrome", "--user-data-dir"],
    ] {
        assert!(
            classify(
                &[p(1, 0, &args)],
                &PathBuf::from("/chrome"),
                &PathBuf::from("/real"),
                &HashSet::new(),
                &norm
            )
            .is_err()
        );
    }
}

#[test]
fn orphan_crashpad_requires_exact_database_root() {
    let mut crash = p(10, 0, &["handler", "--database=/real/Crashpad"]);
    crash.executable = PathBuf::from("/Contents/Helpers/chrome_crashpad_handler");
    let exe = PathBuf::from("/Contents/MacOS/Chrome");
    let root = PathBuf::from("/real");
    assert_eq!(
        classify(&[crash.clone()], &exe, &root, &HashSet::new(), &norm)
            .unwrap()
            .relevant
            .len(),
        1
    );
    crash.args = Some(vec![
        "handler".into(),
        if cfg!(windows) {
            "--database=C:/real-other/Crashpad".into()
        } else {
            "--database=/real-other/Crashpad".into()
        },
    ]);
    assert!(
        classify(&[crash.clone()], &exe, &root, &HashSet::new(), &norm)
            .unwrap()
            .relevant
            .is_empty()
    );
    crash.args = Some(vec!["handler".into()]);
    assert!(classify(&[crash], &exe, &root, &HashSet::new(), &norm).is_err());
}

#[test]
fn reused_direct_and_final_parent_hops_refused() {
    let mut newer = p(1, 0, &["chrome", "--user-data-dir=/temp"]);
    newer.birth = 100;
    let child = p(2, 1, &["chrome", "--type=renderer"]);
    assert!(
        classify(
            &[newer.clone(), child.clone()],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .is_err()
    );
    let mut middle = p(3, 1, &["chrome", "--type=utility"]);
    middle.birth = 5;
    let mut leaf = p(4, 3, &["chrome", "--type=renderer"]);
    leaf.birth = 10;
    assert!(
        classify(
            &[newer, middle, leaf],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .is_err()
    );
}
#[test]
fn foreign_installation_children_cannot_disappear_from_guard() {
    let mut foreign = p(1, 0, &["chrome"]);
    foreign.executable = PathBuf::from("/other/chrome");
    let mut child = p(2, 1, &["chrome", "--type=renderer"]);
    child.executable = foreign.executable.clone();
    assert!(
        classify(
            &[foreign.clone(), child.clone()],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .is_err()
    );
    foreign.args = Some(vec![
        "chrome".into(),
        if cfg!(windows) {
            "--user-data-dir=C:/temp".into()
        } else {
            "--user-data-dir=/temp".into()
        },
    ]);
    assert!(
        classify(
            &[foreign, child.clone()],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .unwrap()
        .relevant
        .is_empty()
    );
    assert!(
        classify(
            &[child],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .is_err()
    );
}
#[test]
fn unreadable_args_and_sibling_root() {
    let mut process = p(1, 0, &["chrome"]);
    process.args = None;
    assert!(
        classify(
            &[process],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .is_err()
    );
    let process = p(1, 0, &["chrome", "--user-data-dir=/real-other"]);
    assert!(
        classify(
            &[process],
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .unwrap()
        .relevant
        .is_empty()
    );
    let rows = vec![
        p(1, 0, &["chrome"]),
        p(2, 1, &["chrome", "--type=crashpad-handler"]),
    ];
    assert_eq!(
        classify(
            &rows,
            &PathBuf::from("/chrome"),
            &PathBuf::from("/real"),
            &HashSet::new(),
            &norm
        )
        .unwrap()
        .relevant
        .len(),
        2
    );
}

#[test]
fn separate_root_argument_keeps_spaces() {
    let value = if cfg!(windows) {
        "C:/root with spaces"
    } else {
        "/root with spaces"
    };
    let process = p(1, 0, &["chrome", "--user-data-dir", value]);
    assert_eq!(
        classify(
            &[process],
            &PathBuf::from("/chrome"),
            &PathBuf::from(value),
            &HashSet::new(),
            &norm
        )
        .unwrap()
        .main
        .len(),
        1
    );
}
