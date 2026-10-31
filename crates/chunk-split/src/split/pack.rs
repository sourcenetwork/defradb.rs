//! Packs pieces into chunks, filling each chunk as far as it goes.

// The pieces are adjacent slices of the text, so joining them gives it back.
pub(super) fn pack(atoms: &[&str], max: usize) -> Vec<String> {
    let mut cores = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0;
    for atom in atoms {
        let len = atom.chars().count();
        if !cur.is_empty() && cur_chars + len > max {
            cores.push(std::mem::take(&mut cur));
            cur_chars = 0;
        }
        cur.push_str(atom);
        cur_chars += len;
    }
    if !cur.is_empty() {
        cores.push(cur);
    }
    cores
}
