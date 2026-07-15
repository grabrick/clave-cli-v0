pub(crate) fn previous_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(crate) fn next_boundary(input: &str, cursor: usize) -> usize {
    input[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .unwrap_or_else(|| input.len())
}

pub(crate) fn previous_word_boundary(input: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut position = cursor;
    while position > 0 {
        let previous = previous_boundary(input, position);
        let ch = input[previous..position].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        position = previous;
    }

    if position == 0 {
        return 0;
    }

    let previous = previous_boundary(input, position);
    let word_mode = is_word_char(input[previous..position].chars().next().unwrap_or(' '));

    while position > 0 {
        let previous = previous_boundary(input, position);
        let ch = input[previous..position].chars().next().unwrap_or(' ');
        if ch.is_whitespace() || is_word_char(ch) != word_mode {
            break;
        }
        position = previous;
    }

    position
}

pub(crate) fn next_word_boundary(input: &str, cursor: usize) -> usize {
    if cursor >= input.len() {
        return input.len();
    }

    let mut position = cursor;
    while position < input.len() {
        let next = next_boundary(input, position);
        let ch = input[position..next].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        position = next;
    }

    if position >= input.len() {
        return input.len();
    }

    let next = next_boundary(input, position);
    let word_mode = is_word_char(input[position..next].chars().next().unwrap_or(' '));

    while position < input.len() {
        let next = next_boundary(input, position);
        let ch = input[position..next].chars().next().unwrap_or(' ');
        if ch.is_whitespace() || is_word_char(ch) != word_mode {
            break;
        }
        position = next;
    }

    position
}

pub(crate) fn line_start_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

pub(crate) fn line_end_boundary(input: &str, cursor: usize) -> usize {
    input[cursor..]
        .find('\n')
        .map(|index| cursor + index)
        .unwrap_or_else(|| input.len())
}

pub(crate) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    // Part 1: is_word_char -- mutants 100:5 (x2), 100:26, 100:32.
    #[test]
    fn is_word_char_classifies_word_and_non_word() {
        // ->false mutant caught: word chars must be true.
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('7'));
        // 100:32 (== -> !=) caught on '_': mutant gives false||false=false.
        assert!(is_word_char('_'));
        // ->true and 100:32 (' ' -> true) caught: non-word chars must be false.
        assert!(!is_word_char(' '));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('-'));
        // 100:26 (|| -> &&) caught on 'a': mutant gives true&&false=false.
    }

    // Part 2: previous_word_boundary.
    #[test]
    fn previous_word_boundary_normal_cases() {
        assert_eq!(previous_word_boundary("hello world", 11), 6);
        assert_eq!(previous_word_boundary("hello world", 6), 0);
        assert_eq!(previous_word_boundary("", 0), 0);
        assert_eq!(previous_word_boundary("abc", 0), 0);
    }

    #[test]
    fn previous_word_boundary_first_loop_skips_whitespace() {
        // 23:20 (> -> == / <): first loop is skipped -> 3 instead of 0.
        // 23:20 (> -> >=): fast on original code; mutant caught by timeout.
        assert_eq!(previous_word_boundary("   ", 3), 0);
        // 26:12 (delete !): mutant breaks whitespace skip -> 2 instead of 0.
        assert_eq!(previous_word_boundary("a ", 2), 0);
    }

    #[test]
    fn previous_word_boundary_second_loop_stops_at_class_change() {
        // 42:31 (|| -> &&): mutant runs past punctuation to 0 instead of 3.
        assert_eq!(previous_word_boundary("ab.cd", 5), 3);
    }

    // Part 3: next_word_boundary (mirror of part 2).
    #[test]
    fn next_word_boundary_normal_cases() {
        assert_eq!(next_word_boundary("hello world", 0), 5);
        assert_eq!(next_word_boundary("hello world", 5), 11);
        assert_eq!(next_word_boundary("abc", 3), 3);
        assert_eq!(next_word_boundary("", 0), 0);
    }

    #[test]
    fn next_word_boundary_first_loop_skips_whitespace() {
        // 57:20 (< -> == / >): first loop is skipped -> 0 instead of 3.
        // 57:20 (< -> <=): fast on original code; mutant caught by timeout.
        assert_eq!(next_word_boundary("   ", 0), 3);
        // 60:12 (delete !): mutant breaks whitespace skip -> 0 instead of 2.
        assert_eq!(next_word_boundary(" a", 0), 2);
    }

    #[test]
    fn next_word_boundary_second_loop_stops_at_class_change() {
        // 76:31 (|| -> &&): mutant runs past punctuation to 5 instead of 2.
        assert_eq!(next_word_boundary("ab.cd", 0), 2);
    }
}
