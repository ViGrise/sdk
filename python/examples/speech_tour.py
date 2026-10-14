"""Walks through the Speech API with the async client: a synthesis, a job that reports to a webhook,
the job's audio, and the verified webhook.

python examples/speech_tour.py <gateway url> <voice>

The gateway must be able to reach 127.0.0.1, where the example listens.
"""

import asyncio
import sys

from vigrise.speech import AsyncSpeechClient, JobText, verify_webhook


async def main(endpoint: str, voice: str) -> None:
  async with AsyncSpeechClient(endpoint) as speech:
    spoken = await speech.synthesize("Xin chào", voice)
    print(f"synthesized {len(spoken.audio)} bytes as {spoken.generation_id}")

    secret = (await speech.rotate_webhook_secret()).secret
    delivered: "asyncio.Future" = asyncio.get_running_loop().create_future()

    async def receive(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
      head = await reader.readuntil(b"\r\n\r\n")
      headers = {}
      for line in head.decode().split("\r\n")[1:]:
        name, _, value = line.partition(":")
        headers[name.strip().lower()] = value.strip()
      body = await reader.readexactly(int(headers.get("content-length", "0")))
      try:
        if not delivered.done():
          delivered.set_result(verify_webhook(secret, headers, body))
        status = "200 OK"
      except Exception:
        status = "400 Bad Request"
      writer.write(f"HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".encode())
      await writer.drain()
      writer.close()

    server = await asyncio.start_server(receive, "127.0.0.1", 0)
    port = server.sockets[0].getsockname()[1]
    async with server:
      job = await speech.create_job(
        JobText("Xin chào các bạn", voice), webhook_url=f"http://127.0.0.1:{port}/hook"
      )
      finished = await speech.wait_for_job(job.id, poll_interval=0.5, timeout=60)
      audio = await speech.download_generation_audio(finished.generation_id)
      print(f"job {finished.id} {finished.status}: {len(audio)} bytes of audio")
      event = await asyncio.wait_for(delivered, 60)
      print(f"webhook {event.type} for {event.data.id}")
    print("tour ok")


asyncio.run(main(sys.argv[1], sys.argv[2]))
