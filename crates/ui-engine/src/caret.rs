//! Caret placement in edited text: character boundaries for a byte caret and for a press.

/// `byte` clamped into `text` and back to a character boundary.
pub fn caret_byte(text: &str, byte: usize) -> usize {
    let mut at = byte.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The character boundary of `text` nearest `x`, from each prefix's drawn width.
pub fn nearest_boundary(text: &str, x: f32, mut width: impl FnMut(&str) -> f32) -> usize {
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect();
    // Widths grow with the prefix: find the first boundary at or past `x`.
    let (mut low, mut high) = (0, bounds.len() - 1);
    while low < high {
        let middle = (low + high) / 2;
        if width(&text[..bounds[middle]]) < x {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low > 0 && x - width(&text[..bounds[low - 1]]) < width(&text[..bounds[low]]) - x {
        low -= 1;
    }
    bounds[low]
}
