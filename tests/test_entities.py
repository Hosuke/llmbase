"""Tests for entity extraction and dedup."""

from tools.entities import _dedup_entities, _parse_entity_response


def test_dedup_by_name():
    people = [
        {"name": "Confucius", "name_local": "孔子", "dates": "551 BCE", "articles": ["a1"]},
        {"name": "Confucius", "name_local": "孔子", "dates": "c.551-479 BCE", "articles": ["a2"]},
    ]
    result = _dedup_entities(people)
    assert len(result) == 1
    assert set(result[0]["articles"]) == {"a1", "a2"}
    assert result[0]["dates"] == "c.551-479 BCE"  # longer wins


def test_dedup_by_name_local():
    people = [
        {"name": "Confucius", "articles": ["a1"]},
        {"name_local": "孔子", "articles": ["a2"]},
        {"name": "Confucius", "name_local": "孔子", "articles": ["a3"]},
    ]
    result = _dedup_entities(people)
    assert len(result) == 1
    assert len(result[0]["articles"]) == 3


def test_dedup_null_articles():
    people = [
        {"name": "X", "articles": ["a1"]},
        {"name": "X", "articles": None},
    ]
    result = _dedup_entities(people)
    assert len(result) == 1
    assert result[0]["articles"] == ["a1"]


def test_dedup_non_dict_items():
    entities = [{"name": "X", "articles": []}, "garbage", None, 42]
    result = _dedup_entities(entities)
    assert len(result) == 1


def test_dedup_events_date_field():
    events = [
        {"name": "Battle", "date": "200 BCE", "articles": ["e1"]},
        {"name": "Battle", "date": "200-199 BCE", "articles": ["e2"]},
    ]
    result = _dedup_entities(events)
    assert len(result) == 1
    assert result[0]["date"] == "200-199 BCE"


def test_dedup_empty():
    assert _dedup_entities([]) == []
    assert _dedup_entities(None) == []


def test_parse_entity_response_valid():
    response = '{"people": [{"name": "X"}], "events": [], "places": []}'
    result = _parse_entity_response(response)
    assert len(result["people"]) == 1


def test_parse_entity_response_with_thinking():
    response = 'Let me think about this...\n\n{"people": [], "events": [{"name": "Y"}], "places": []}'
    result = _parse_entity_response(response)
    assert len(result["events"]) == 1


def test_parse_entity_response_invalid():
    result = _parse_entity_response("not json at all")
    assert result["people"] == []
    assert result["events"] == []
    assert result["places"] == []
