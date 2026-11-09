"""Any server with the OpenAI embeddings API (POST <url>/embeddings): a
hosted provider, Ollama, llama.cpp. Vectors are normalized to length 1."""

import json
import math
import os
import urllib.request


class Backend:
    def __init__(self, url, model, key):
        self.url = url.rstrip("/") + "/embeddings"
        self.model = model
        self.key = key

    def embed(self, texts):
        body = json.dumps({"model": self.model, "input": texts}).encode()
        headers = {"Content-Type": "application/json"}
        if self.key:
            headers["Authorization"] = f"Bearer {self.key}"
        request = urllib.request.Request(self.url, data=body, headers=headers)
        with urllib.request.urlopen(request, timeout=60) as response:
            data = json.load(response)["data"]
        vectors = [item["embedding"] for item in sorted(data, key=lambda d: d["index"])]
        return [[x / (math.sqrt(sum(v * v for v in vector)) or 1) for x in vector] for vector in vectors]


def load(settings):
    if not settings.url or not settings.model:
        raise SystemExit("embed.toml: the openai backend needs url and model")
    key = os.environ.get(settings.api_key_env, "") if settings.api_key_env else ""
    return Backend(settings.url, settings.model, key)
