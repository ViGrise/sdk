"""Lists the voices, or synthesizes "Xin chào", through the Speech API.

python examples/speech.py <gateway url> [voice]

Prints the HTTP status, then the number of voices or the bytes of audio.
Credentials come from the environment or ~/.vigrise/credentials.
"""

import sys

from vigrise.speech import AudioFormat, SpeechClient, SpeechServiceError

endpoint = sys.argv[1]
voice = sys.argv[2] if len(sys.argv) > 2 else None
with SpeechClient(endpoint) as speech:
    try:
        if voice:
            print(200, len(speech.synthesize("Xin chào", voice, format=AudioFormat.WAV).audio))
        else:
            print(200, len(speech.list_voices()))
    except SpeechServiceError as error:
        print(error.status, error.code)
