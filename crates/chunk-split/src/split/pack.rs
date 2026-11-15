//! Packs pieces into chunks, filling each chunk as far as it goes.

/// One piece of a segment, and whether it is the start of a heading.
#[derive(Debug, Clone, Copy)]
pub(super) struct Atom<'a> {
    pub text: &'a str,
    pub heading: bool,
}

// The pieces are adjacent slices of the text, so joining them gives it back.
pub(super) fn pack(atoms: &[Atom<'_>], max: usize) -> Vec<String> {
    let mut cores = Vec::new();
    let mut cur = String::new();
    let mut cur_chars = 0;
    let mut after_heading = false;
    for (i, atom) in atoms.iter().enumerate() {
        let len = atom.text.chars().count();
        let full =
            cur_chars + len > max || (!after_heading && breaks_before(&atoms[i..], cur_chars, max));
        if !cur.is_empty() && full {
            cores.push(std::mem::take(&mut cur));
            cur_chars = 0;
        }
        cur.push_str(atom.text);
        cur_chars += len;
        after_heading = atom.heading;
    }
    if !cur.is_empty() {
        cores.push(cur);
    }
    cores
}

// A heading starts a new chunk when the current one is half full, or when the
// heading and the text right under it would not fit, so it stays with that text.
fn breaks_before(rest: &[Atom<'_>], cur_chars: usize, max: usize) -> bool {
    rest[0].heading && (2 * cur_chars >= max || cur_chars + unit_chars(rest) > max)
}

// A heading stays with the text under it, so both count as one unit.
fn unit_chars(rest: &[Atom<'_>]) -> usize {
    let text = rest
        .iter()
        .position(|a| !a.heading)
        .unwrap_or(rest.len() - 1);
    rest[..=text].iter().map(|a| a.text.chars().count()).sum()
}
