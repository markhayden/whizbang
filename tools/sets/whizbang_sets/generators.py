"""The generators behind one small interface: `generate(prompt, seed, size)` for a new
picture, `edit(image, prompt, seed)` for one instruction on one picture, and
`compose(images, prompt, seed)` for a new scene that carries the subject of one or more
references. Each returns PNG or JPEG bytes.

The three live ones are squint's: FLUX.2 [klein] 4B on fal, gpt-image-2 through OpenAI's
Images API, and Gemini 3.1 Flash Image through Google's API, whose providers are recipients
the owner approved. Every call goes through a `Sender` (`egress.py`). A key is read from the
environment by its name (FAL_KEY, OPENAI_API_KEY, GEMINI_API_KEY) when a call is made, and
is never written anywhere.

Ported from squint `scripts/sets/src/squint_sets/generators.py`, reshaped: the calls are
checked, and fal is asked to answer with the picture itself (`sync_mode`), so that usually
nothing is downloaded from another of its hosts.
"""

from __future__ import annotations

import base64
import io
import json
import os
import uuid

from PIL import Image, ImageDraw

from .egress import GeneratorError, Sender

PROMPT = ("prompts",)
PROMPT_AND_PICTURES = ("prompts", "source_images")


def _key(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise GeneratorError(f"{name} is not set")
    return value


def _mime(image: bytes) -> str:
    return "image/png" if image[:8] == b"\x89PNG\r\n\x1a\n" else "image/jpeg"


def _data_address(image: bytes) -> str:
    return f"data:{_mime(image)};base64," + base64.b64encode(image).decode()


def _json(sender: Sender, url: str, body: dict, credentials: dict, classes, what: str):
    answer, recipient = sender.post(
        url, json.dumps(body).encode(), {"Content-Type": "application/json"}, credentials, classes, what
    )
    try:
        return json.loads(answer), recipient
    except ValueError:
        raise GeneratorError(f"{what}: the answer is not JSON") from None


class FalKlein:
    """FLUX.2 [klein] 4B on fal: text to picture, instruction edits and scenes from several
    references, all seeded."""

    name = "fal"
    model = "fal-ai/flux-2-klein/4b"
    edit_model = "fal-ai/flux-2/klein/4b/edit"
    base = "https://fal.run"

    def __init__(self, sender: Sender):
        self.sender = sender

    def _call(self, model: str, body: dict, classes) -> bytes:
        d, recipient = _json(
            self.sender,
            f"{self.base}/{model}",
            {**body, "sync_mode": True},
            {"Authorization": "Key " + _key("FAL_KEY")},
            classes,
            model,
        )
        images = d.get("images") or []
        if not images or not isinstance(images[0].get("url"), str):
            raise GeneratorError(f"{model}: no picture in the answer")
        url = images[0]["url"]
        if url.startswith("data:"):
            try:
                return base64.b64decode(url.split(",", 1)[1], validate=True)
            except (IndexError, ValueError):
                raise GeneratorError(f"{model}: the picture in the answer is not base64") from None
        return self.sender.get(url, recipient, f"{model} picture")

    def generate(self, prompt: str, seed: int, size: int) -> bytes:
        body = {
            "prompt": prompt,
            "seed": seed,
            "image_size": {"width": size, "height": size},
            "num_images": 1,
            "output_format": "png",
        }
        return self._call(self.model, body, PROMPT)

    def edit(self, image: bytes, prompt: str, seed: int) -> bytes:
        return self.compose([image], prompt, seed)

    def compose(self, images: list[bytes], prompt: str, seed: int) -> bytes:
        body = {
            "prompt": prompt,
            "image_urls": [_data_address(i) for i in images],
            "seed": seed,
            "num_images": 1,
            "output_format": "png",
        }
        return self._call(self.edit_model, body, PROMPT_AND_PICTURES)


class OpenAiImage:
    """gpt-image-2 through the Images API. It takes no seed, so the record keeps the prompt
    and the date."""

    name = "openai"
    model = "gpt-image-2"
    base = "https://api.openai.com/v1/images"

    def __init__(self, sender: Sender):
        self.sender = sender

    @staticmethod
    def _picture(d: dict, what: str) -> bytes:
        try:
            return base64.b64decode(d["data"][0]["b64_json"])
        except (KeyError, IndexError, TypeError, ValueError):
            raise GeneratorError(f"{what}: no picture in the answer") from None

    def generate(self, prompt: str, seed: int, size: int) -> bytes:
        body = {
            "model": self.model,
            "prompt": prompt,
            "size": f"{size}x{size}",
            "quality": "medium",
            "output_format": "png",
            "n": 1,
        }
        key = {"Authorization": "Bearer " + _key("OPENAI_API_KEY")}
        d, _ = _json(self.sender, f"{self.base}/generations", body, key, PROMPT, "images/generations")
        return self._picture(d, "images/generations")

    def edit(self, image: bytes, prompt: str, seed: int) -> bytes:
        return self.compose([image], prompt, seed)

    def compose(self, images: list[bytes], prompt: str, seed: int) -> bytes:
        boundary = "----whizbang" + uuid.uuid4().hex
        parts = []
        for name, value in (
            ("model", self.model),
            ("prompt", prompt),
            ("quality", "medium"),
            ("output_format", "png"),
        ):
            parts.append(
                f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode()
            )
        for n, image in enumerate(images, start=1):
            head = (
                f'--{boundary}\r\nContent-Disposition: form-data; name="image[]"; '
                f'filename="input-{n}.png"\r\nContent-Type: {_mime(image)}\r\n\r\n'
            )
            parts.append(head.encode() + image + b"\r\n")
        parts.append(f"--{boundary}--\r\n".encode())
        answer, _ = self.sender.post(
            f"{self.base}/edits",
            b"".join(parts),
            {"Content-Type": f"multipart/form-data; boundary={boundary}"},
            {"Authorization": "Bearer " + _key("OPENAI_API_KEY")},
            PROMPT_AND_PICTURES,
            "images/edits",
        )
        try:
            d = json.loads(answer)
        except ValueError:
            raise GeneratorError("images/edits: the answer is not JSON") from None
        return self._picture(d, "images/edits")


class GeminiImage:
    """Gemini 3.1 Flash Image through generateContent. It takes no seed."""

    name = "gemini"
    model = "gemini-3.1-flash-image"
    base = "https://generativelanguage.googleapis.com/v1beta/models"

    def __init__(self, sender: Sender):
        self.sender = sender

    def _call(self, parts: list[dict], classes) -> bytes:
        body = {
            "contents": [{"parts": parts}],
            "generationConfig": {
                "responseModalities": ["IMAGE"],
                "imageConfig": {"aspectRatio": "1:1"},
            },
        }
        d, _ = _json(
            self.sender,
            f"{self.base}/{self.model}:generateContent",
            body,
            {"x-goog-api-key": _key("GEMINI_API_KEY")},
            classes,
            self.model,
        )
        candidates = d.get("candidates") or [{}]
        for p in candidates[0].get("content", {}).get("parts", []):
            if "inlineData" in p:
                return base64.b64decode(p["inlineData"]["data"])
        raise GeneratorError(f"{self.model}: no picture part in the answer")

    def generate(self, prompt: str, seed: int, size: int) -> bytes:
        return self._call([{"text": prompt}], PROMPT)

    def edit(self, image: bytes, prompt: str, seed: int) -> bytes:
        return self.compose([image], prompt, seed)

    def compose(self, images: list[bytes], prompt: str, seed: int) -> bytes:
        parts: list[dict] = [
            {"inlineData": {"mimeType": _mime(i), "data": base64.b64encode(i).decode()}}
            for i in images
        ]
        parts.append({"text": prompt})
        return self._call(parts, PROMPT_AND_PICTURES)


def _png(im: Image.Image) -> bytes:
    buf = io.BytesIO()
    im.save(buf, format="PNG")
    return buf.getvalue()


class FakeGenerator:
    """Pictures made here, the same every time, for tests and dry runs with no network: a
    coloured scene with a few shapes; `edit` paints over one patch, so the region of the
    difference is plain; `compose` carries the first reference's subject onto a new ground."""

    name = "fake"
    model = "fake-model"

    def generate(self, prompt: str, seed: int, size: int) -> bytes:
        im = Image.new("RGB", (size, size), (120 + seed % 60, 140, 160))
        d = ImageDraw.Draw(im)
        d.rectangle((size * 0.3, size * 0.4, size * 0.7, size * 0.9), fill=(170, 60, 50))
        d.rectangle((size * 0.45, size * 0.65, size * 0.55, size * 0.9), fill=(20, 80, 40))
        d.ellipse((size * 0.05, size * 0.1, size * 0.3, size * 0.5), fill=(60, 120, 50))
        d.ellipse((size * 0.72, size * 0.8, size * 0.9, size * 0.9), fill=(40, 60, 200))
        return _png(im)

    def edit(self, image: bytes, prompt: str, seed: int) -> bytes:
        im = Image.open(io.BytesIO(image)).convert("RGB")
        w, h = im.size
        ImageDraw.Draw(im).rectangle((w * 0.7, h * 0.78, w * 0.92, h * 0.92), fill=(120, 140, 160))
        return _png(im)

    def compose(self, images: list[bytes], prompt: str, seed: int) -> bytes:
        first = Image.open(io.BytesIO(images[0])).convert("RGB")
        w, h = first.size
        im = Image.new("RGB", (w, h), (90 + seed % 80, 70, 110))
        im.paste(
            first.crop((int(w * 0.3), int(h * 0.4), int(w * 0.7), int(h * 0.9))),
            (int(w * 0.2), int(h * 0.3)),
        )
        return _png(im)


def live(sender: Sender) -> dict:
    return {
        "fal": FalKlein(sender),
        "openai": OpenAiImage(sender),
        "gemini": GeminiImage(sender),
        "fake": FakeGenerator(),
    }


def fake() -> dict:
    return {name: FakeGenerator() for name in ("fal", "openai", "gemini", "fake")}
