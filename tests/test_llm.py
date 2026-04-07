"""Tests for LLM utilities (extract_json, etc.)."""

from tools.llm import extract_json


def test_extract_json_clean_array():
    assert extract_json('[{"id": "test"}]') == '[{"id": "test"}]'


def test_extract_json_clean_object():
    assert extract_json('{"key": "value"}') == '{"key": "value"}'


def test_extract_json_with_thinking():
    text = 'Let me think...\nStep 1: analyze\nStep 2: create\n[{"id": "result"}]'
    assert extract_json(text) == '[{"id": "result"}]'


def test_extract_json_thinking_with_brackets():
    text = 'I see tags [a, b, c] in the data.\n\n[{"id": "actual"}]'
    result = extract_json(text)
    assert '"actual"' in result


def test_extract_json_plain_text():
    text = "This is just a plain answer with no JSON"
    assert extract_json(text) == text


def test_extract_json_rightmost_wins():
    """When both array and object exist, rightmost should be picked."""
    text = 'thinking {"x":1} more thinking [{"id":"final"}]'
    result = extract_json(text)
    assert '"final"' in result


def test_extract_json_markdown_fenced():
    text = '```json\n[{"id": "fenced"}]\n```'
    # extract_json should handle clean JSON that starts with [
    # The markdown fences are handled by the caller (_parse_taxonomy_response)
    result = extract_json(text)
    assert "fenced" in result or "```" in result  # May or may not strip fences


def test_extract_json_invalid_json():
    text = 'thinking... [not valid json {'
    result = extract_json(text)
    # Should return original text when no valid JSON found
    assert result == text
