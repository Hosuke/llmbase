"""Adversarial public-facade callback, trace and parser-override checks."""
import hashlib
import json
from pathlib import Path
import sys

from llmwiki import _core, _native, normalize

assert Path(_core.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
before_roots = dict(_native._configured_roots)
body = '# outside\n```\n# skip\n```\n<div>\n# skip2\n</div>\n\n## another\n'
rules = [{'pattern': 'never', 'level': 2}, {'pattern': 'match', 'level': 4}]
compiled, searched = [], []
original_compile = normalize.re.compile


class Pattern:
    def __init__(self, name):
        self.name = name

    def search(self, title):
        searched.append((self.name, title))
        return self.name == 'match'


def compile_pattern(pattern):
    compiled.append(pattern)
    return Pattern(pattern)


try:
    normalize.re.compile = compile_pattern
    actual = normalize.normalize_heads(body, rules)
finally:
    normalize.re.compile = original_compile
assert compiled == ['never', 'match']
assert searched == [('never', 'outside'), ('match', 'outside'), ('never', 'another'), ('match', 'another')]
assert actual == body.replace('# outside', '#### outside').replace('## another', '#### another')
trace = _native.last_trace()
assert trace['core'] == 'rust' and trace['matcher'] == 'python_regex'
assert trace.get('callback_count', trace.get('callback_calls')) == 2, trace

failure = TypeError('primary matcher exception identity')
calls = []


class BrokenPattern:
    def search(self, title):
        calls.append(title)
        # Reentrant native success must not remain after this callback fails.
        _native.utility_transform('sha1', {'text': 'nested successful call'})
        raise failure


try:
    normalize.re.compile = lambda pattern: BrokenPattern()
    try:
        normalize.normalize_heads(body, rules)
    except TypeError as error:
        assert error is failure and calls == ['outside']
        assert _native.last_trace() is None
    else:
        raise AssertionError('matcher exception swallowed')
finally:
    normalize.re.compile = original_compile

original_rewrite = normalize._rewrite_heading_level
rewrites = []


def rewrite(line, level):
    rewrites.append((line, level))
    return 'OVERRIDE'


try:
    normalize._rewrite_heading_level = rewrite
    assert normalize.normalize_heads('# title\n', [{'pattern': '.', 'level': 2}]) == 'OVERRIDE\n'
    assert rewrites == [('# title', 2)]
    assert _native.last_trace()['core'] == 'python'
finally:
    normalize._rewrite_heading_level = original_rewrite
assert normalize.normalize_heads('# title\n', [{'pattern': '.', 'level': 2}]) == '## title\n'
assert _native.last_trace()['core'] == 'rust'
assert _native._configured_roots == before_roots
receipt = {'passed': True, 'native_callback_count': 2, 'matcher_calls': len(searched),
           'exception_identity_and_no_retry': True, 'reentrant_trace_cleared': True,
           'parser_override_and_restoration': True, 'no_library_registration': True,
           'extension_sha256': hashlib.sha256(Path(_core.__file__).read_bytes()).hexdigest()}
Path(__file__).with_name('callbacks-result.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
