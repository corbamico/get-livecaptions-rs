use super::*;

#[test]
fn test_extract_new_lines_first_run() {
    let previous = "";
    let current = "Hello world\nThis is new";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "Hello world\nThis is new");
}

#[test]
fn test_extract_new_lines_normal_accumulation() {
    let previous = "Hello world\nThis is existing";
    let current = "Hello world\nThis is existing\nThis is new line";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "This is new line\n");
}

#[test]
fn test_extract_new_lines_no_new_content() {
    let previous = "Hello world\nSame text";
    let current = "Hello world\nSame text";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "");
}

#[test]
fn test_extract_new_lines_complete_truncation() {
    let previous = "Old content\nThat is gone";
    let current = "Completely new\nDifferent text";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "Completely new\nDifferent text");
}

#[test]
fn test_extract_new_lines_partial_overlap() {
    let previous = "Line 1\nLine 2\nLine 3";
    let current = "Line 2\nLine 3\nLine 4\nLine 5";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "Line 4\nLine 5\n");
}

#[test]
fn test_extract_new_lines_partial_overlap2() {
    let previous = "Line 1\nLine 2\nLine 3";
    let current = "Line 2\nLine 3 Line 3\nLine 4\nLine 5";

    let result = extract_new_lines(previous, current);
    // Finds "Line 2" as overlap (1 line match), returns content after it
    // "Line 3" doesn't match "Line 3 Line 3", so match stops at 1 line
    assert_eq!(result, "Line 3 Line 3\nLine 4\nLine 5\n");
}

#[test]
fn test_extract_new_lines_empty_previous_lines() {
    let previous = "\n\n";
    let current = "New content here";

    let result = extract_new_lines(previous, current);
    assert!(result.contains("New content here"));
}

#[test]
fn test_extract_new_lines_multiple_new_lines() {
    let previous = "First line";
    let current = "First line\nSecond line\nThird line\nFourth line";

    let result = extract_new_lines(previous, current);
    assert_eq!(result, "Second line\nThird line\nFourth line\n");
}
