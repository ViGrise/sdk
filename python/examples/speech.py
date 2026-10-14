"""Calls the ViGrise Speech API with a VGR4 signature.

python examples/speech.py <gateway url> [voice]
"""

import sys

import requests

from vigrise.auth.requests_auth import ViGriseAuth

base = sys.argv[1]
voice = sys.argv[2] if len(sys.argv) > 2 else None
auth = ViGriseAuth("speech")
if voice:
    response = requests.post(f"{base}/v1/audio/speech", json={"input": "Xin chào", "voice": voice, "response_format": "wav"}, auth=auth)
else:
    response = requests.get(f"{base}/v1/voices", auth=auth)
print(response.status_code, len(response.content))
