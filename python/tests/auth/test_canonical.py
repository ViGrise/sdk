import pytest

from vigrise.auth.canonical import canonical_headers, canonical_query, canonical_uri


def test_escapes_never_raise():
    assert canonical_uri("/a%ff/%zz/%4") == "/a%FF/%25zz/%254"
    assert canonical_query("k=%e1%bb%8d&k=%") == "k=%25&k=%E1%BB%8D"


def test_plus_is_literal_and_raw_unicode_is_utf8():
    assert canonical_query("q=a+b") == "q=a%2Bb"
    assert canonical_uri("/giọng") == "/gi%E1%BB%8Dng"


def test_repeated_headers_and_missing_signed_header():
    assert canonical_headers([("X-Vgr-Meta", " a  b "), ("x-vgr-meta", "c")], ["x-vgr-meta"]) == "x-vgr-meta:a b,c\n"
    with pytest.raises(ValueError, match="host"):
        canonical_headers([], ["host"])
