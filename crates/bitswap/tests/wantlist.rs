use bitswap::server::wantlist::Wantlist;
use bitswap::WantType;
use cid::Cid;

fn cids() -> Vec<Cid> {
    [
        "QmQL8LqkEgYXaDHdNYCG2mmpow7Sp8Z8Kt3QS688vyBeC7",
        "QmcBDsdjgSXU7BP4A4V8LJCXENE5xVwnhrhRGVTJr9YCVj",
        "QmQakgd2wDxc3uUF4orGdEm28zUT9Mmimp5pyPG2SFS9Gj",
    ]
    .into_iter()
    .map(|c| Cid::try_from(c).unwrap())
    .collect()
}

#[test]
fn basic() {
    let cids = cids();
    let mut wl = Wantlist::default();

    assert!(wl.add(cids[0], 5, WantType::Block));
    assert!(wl.add(cids[1], 4, WantType::Block));
    assert_eq!(wl.len(), 2);
    assert!(!wl.add(cids[1], 4, WantType::Block));
    assert_eq!(wl.len(), 2);

    assert!(wl.remove_type(&cids[0], WantType::Block).is_some());
    assert_eq!(wl.get(&cids[1]).unwrap().cid, cids[1]);
    assert!(wl.get(&cids[0]).is_none());
}

#[test]
fn have_then_block_upgrades() {
    let cids = cids();
    let mut wl = Wantlist::default();
    assert!(wl.add(cids[0], 5, WantType::Have));
    assert!(wl.add(cids[0], 5, WantType::Block));
    assert_eq!(wl.len(), 1);
    assert_eq!(wl.get(&cids[0]).unwrap().want_type, WantType::Block);
}

#[test]
fn block_then_have_does_not_downgrade() {
    let cids = cids();
    let mut wl = Wantlist::default();
    assert!(wl.add(cids[0], 5, WantType::Block));
    assert!(!wl.add(cids[0], 5, WantType::Have));
    assert_eq!(wl.get(&cids[0]).unwrap().want_type, WantType::Block);
}

#[test]
fn remove_type_respects_type() {
    let cids = cids();
    let mut wl = Wantlist::default();

    assert!(wl.add(cids[0], 5, WantType::Have));
    assert!(wl.remove_type(&cids[0], WantType::Block).is_some());
    assert!(wl.is_empty());

    assert!(wl.add(cids[0], 5, WantType::Block));
    assert!(wl.remove_type(&cids[0], WantType::Have).is_none());
    assert_eq!(wl.len(), 1);

    assert!(wl.remove(&cids[0]).is_some());
    assert!(wl.is_empty());
}

#[test]
fn entries_sort_by_descending_priority() {
    let cids = cids();
    let mut wl = Wantlist::default();
    wl.add(cids[0], 3, WantType::Block);
    wl.add(cids[1], 5, WantType::Have);
    wl.add(cids[2], 4, WantType::Have);

    let order: Vec<Cid> = wl.entries().into_iter().map(|e| e.cid).collect();
    assert_eq!(order, [cids[1], cids[2], cids[0]]);

    wl.remove(&cids[1]);
    assert_eq!(wl.entries().len(), 2);
    wl.clear();
    assert!(wl.entries().is_empty());
}
