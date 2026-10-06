mod common;

use common::fixtures::{canonical, decode, load, s, sha256_hex};
use common::probe::{run, Rx};
use serde_json::Value;

fn frame_mismatch(expected: &Value, ours: &[u8]) -> Option<String> {
    if let Some(golden) = expected["frame_hex"].as_str() {
        let got = hex::encode(ours);
        return (got != golden)
            .then(|| format!("frame bytes differ\n  golden: {golden}\n  ours:   {got}"));
    }
    let (len, digest) = (
        expected["frame_len"].as_u64().unwrap(),
        s(expected, "frame_sha256"),
    );
    let got = sha256_hex(ours);
    (ours.len() as u64 != len || got != digest).then(|| {
        format!(
            "frame differs\n  golden: len {len} sha256 {digest}\n  ours:   len {} sha256 {got}",
            ours.len()
        )
    })
}

fn decoded(rx: &Rx) -> Result<Value, String> {
    decode(&rx.protocol, &rx.frame)
        .map(|m| canonical(&m))
        .map_err(|e| e.to_string())
}

fn compare_step(name: &str, index: usize, expected: &[Value], got: &[Rx], out: &mut Vec<String>) {
    let at = format!("{name} step {index}");
    if expected.len() != got.len() {
        let seen: Vec<String> = got
            .iter()
            .map(|r| format!("{}#{}: {:?}", r.protocol, r.seq, decoded(r)))
            .collect();
        out.push(format!(
            "{at} responses: expected {} frames, got {}\n  ours: {seen:?}",
            expected.len(),
            got.len()
        ));
        return;
    }
    let mut ours: Vec<(&Rx, Result<Value, String>)> = got.iter().map(|r| (r, decoded(r))).collect();
    ours.sort_by_key(|(r, _)| r.seq);
    let mut taken = vec![false; ours.len()];
    let mut order = Vec::new();
    for (i, want) in expected.iter().enumerate() {
        let hit = ours.iter().enumerate().position(|(j, (r, m))| {
            !taken[j]
                && r.protocol == s(want, "protocol")
                && m.as_ref().is_ok_and(|m| *m == want["message"])
        });
        let Some(j) = hit else {
            out.push(format!(
                "{at} response {i} ({}): no frame with this protocol and message\n  golden message: {}\n  ours: {:?}",
                s(want, "protocol"),
                want["message"],
                ours.iter().map(|(r, m)| (r.protocol.clone(), m.clone())).collect::<Vec<_>>()
            ));
            continue;
        };
        taken[j] = true;
        order.push(j);
        if want["exact"].as_bool().unwrap() {
            if let Some(d) = frame_mismatch(want, &ours[j].0.frame) {
                out.push(format!("{at} response {i} exact: {d}"));
            }
        }
    }
    if order.windows(2).any(|w| w[0] > w[1]) {
        eprintln!("{at}: responses matched in a different stream order than recorded: {order:?}");
    }
}

async fn replay(name: &str) {
    let scenario = load("server.json")
        .into_iter()
        .find(|c| s(c, "name") == name)
        .unwrap_or_else(|| panic!("no scenario {name}"));
    let runs = run(&scenario).await;
    let mut out = Vec::new();
    for (i, (step, run)) in scenario["steps"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&runs)
        .enumerate()
    {
        if let Some(e) = &run.error {
            out.push(format!("{name} step {i}: probe error: {e}"));
        }
        compare_step(
            name,
            i,
            step["responses"].as_array().unwrap(),
            &run.responses,
            &mut out,
        );
    }
    assert!(out.is_empty(), "{}", out.join("\n"));
}

macro_rules! scenarios {
    ($($test:ident => $name:literal),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn $test() {
            replay($name).await;
        }
    )*};
}

scenarios! {
    s01 => "01_want_have_S_sdh",
    s02 => "02_want_have_E1024_sdh",
    s03 => "03_want_have_E1025_sdh",
    s04 => "04_want_block_L_sdh",
    s05 => "05_want_have_L_no_sdh",
    s06 => "06_want_have_X_sdh",
    s07 => "07_want_have_X_no_sdh",
    s08 => "08_want_block_X_sdh",
    s09 => "09_want_block_X_no_sdh",
    s10 => "10_want_block_D_denied_sdh",
    s11 => "11_want_block_D_denied_no_sdh",
    s12 => "12_want_have_D_denied_sdh",
    s13 => "13_mixed_wants",
    s14 => "14_want_block_H1_H2_H3",
    s15 => "15_two_frames_same_stream",
    s16 => "16_want_block_L_then_cancel_new_stream",
    s17 => "17_full_wantlist_twice",
    s18a => "18a_v100_want_block_cidv1",
    s18b => "18b_v100_want_block_cidv0",
    s19 => "19_v110_want_have_E1025_sdh",
    s20 => "20_probe_accepts_only_v100",
    s21 => "21_probe_accepts_only_legacy",
    s22 => "22_oversized_frame_then_valid",
    s23 => "23_empty_message",
    s24 => "24_priority_order_S5_C10",
}

#[test]
fn every_fixture_scenario_has_a_test() {
    let covered = [
        "01_want_have_S_sdh",
        "02_want_have_E1024_sdh",
        "03_want_have_E1025_sdh",
        "04_want_block_L_sdh",
        "05_want_have_L_no_sdh",
        "06_want_have_X_sdh",
        "07_want_have_X_no_sdh",
        "08_want_block_X_sdh",
        "09_want_block_X_no_sdh",
        "10_want_block_D_denied_sdh",
        "11_want_block_D_denied_no_sdh",
        "12_want_have_D_denied_sdh",
        "13_mixed_wants",
        "14_want_block_H1_H2_H3",
        "15_two_frames_same_stream",
        "16_want_block_L_then_cancel_new_stream",
        "17_full_wantlist_twice",
        "18a_v100_want_block_cidv1",
        "18b_v100_want_block_cidv0",
        "19_v110_want_have_E1025_sdh",
        "20_probe_accepts_only_v100",
        "21_probe_accepts_only_legacy",
        "22_oversized_frame_then_valid",
        "23_empty_message",
        "24_priority_order_S5_C10",
    ];
    let names: Vec<String> = load("server.json")
        .iter()
        .map(|c| s(c, "name").to_string())
        .collect();
    assert_eq!(names, covered);
}
