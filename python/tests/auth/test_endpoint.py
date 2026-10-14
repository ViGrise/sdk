from vigrise.auth.endpoint import env_key, resolve_endpoint, sts_base_url, sts_url

ENV = {"VIGRISE_ENDPOINT_URL_SPEECH": "http://speech.env", "VIGRISE_ENDPOINT_URL": "http://all.env"}


def test_the_explicit_endpoint_wins_then_the_service_variable_then_the_shared_one():
    assert resolve_endpoint("speech", "http://explicit/", ENV) == "http://explicit"
    assert resolve_endpoint("speech", None, ENV) == "http://speech.env"
    assert resolve_endpoint("storage", None, ENV) == "http://all.env"


def test_the_default_is_the_api_host_of_the_service():
    assert resolve_endpoint("speech", None, {}) == "https://speech-api.vigrise.com"
    assert resolve_endpoint("storage", None, {}) == "https://storage-api.vigrise.com"


def test_empty_values_are_ignored():
    env = {"VIGRISE_ENDPOINT_URL_SPEECH": "", "VIGRISE_ENDPOINT_URL": ""}
    assert resolve_endpoint("speech", "", env) == "https://speech-api.vigrise.com"


def test_the_variable_name_follows_the_service_name():
    assert env_key("speech") == "VIGRISE_ENDPOINT_URL_SPEECH"
    assert env_key("voice-studio") == "VIGRISE_ENDPOINT_URL_VOICE_STUDIO"


def test_the_process_environment_is_the_default_source(monkeypatch):
    monkeypatch.setenv("VIGRISE_ENDPOINT_URL_SPEECH", "http://from-process")
    assert resolve_endpoint("speech") == "http://from-process"


def test_the_sts_url_ends_in_the_assume_role_path():
    assert sts_url(None, {}) == "https://sts-api.vigrise.com/v1/assume-role"
    assert sts_url(None, {"VIGRISE_ENDPOINT_URL": "http://local:8080"}) == "http://local:8080/v1/assume-role"
    assert sts_url(None, {"VIGRISE_ENDPOINT_URL_STS": "http://sts.internal/v1/assume-role"}) == "http://sts.internal/v1/assume-role"
    assert sts_url("http://explicit", {}) == "http://explicit/v1/assume-role"
    assert sts_base_url(None, {"VIGRISE_ENDPOINT_URL_STS": "http://sts.internal/v1/assume-role"}) == "http://sts.internal"
