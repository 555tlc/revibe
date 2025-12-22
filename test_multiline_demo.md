# Multi-line Input Fix Demonstration

## Problem
The multi-line input in the Rust port of mistral-vibe was broken - it didn't get taller when the prompt exceeded 1 line.

## Root Cause
The input area had a fixed height of 3 lines (`Constraint::Length(3)`) in the layout calculation, regardless of how many lines the input content had.

## Solution
1. **Added `calculate_input_height()` function**: This function calculates the required height based on the number of lines in the input content.
   - Empty input: 3 lines (top border + placeholder + bottom border)
   - Single line: 3 lines (top border + content + bottom border)
   - Multiple lines: 2 + line_count lines (top border + content lines + bottom border)

2. **Modified layout calculation**: Changed from fixed height to dynamic height based on content.
   - Before: `Constraint::Length(3)`
   - After: `Constraint::Length(input_height)` where `input_height = calculate_input_height()`

3. **Updated reserved height calculation**: The reserved height is now calculated dynamically based on the actual input height.

## Test Results
The fix has been tested with the following cases:
- Empty input: 3 lines ✓
- Single line: 3 lines ✓
- Two lines: 4 lines ✓
- Three lines: 5 lines ✓
- Four lines: 6 lines ✓

## Code Changes
- `crates/revibe-cli/src/ui/app.rs`: Added `calculate_input_height()` and modified layout calculation
- `crates/revibe-cli/src/ui/input.rs`: Added `set_content()` method for testing
- All existing tests continue to pass
- New test `test_input_height_calculation` verifies the functionality

## Verification
The fix ensures that:
1. The input area grows taller as more lines are added
2. The cursor positioning works correctly for multi-line input
3. The layout remains responsive and doesn't break existing functionality
4. The input box maintains proper borders and styling regardless of height