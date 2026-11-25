"""The generators' egress (plan mE, task B1, item 2): a request or a download goes only to an
origin the config approves for what it carries, a redirect only to such an origin, and a key
never follows a redirect. Two small servers on this machine stand in for the providers; the
tokens are made up. Retries ported from squint `scripts/sets/tests/test_retry.py`."""

import base64
import http.server
import io
import json
import os
import threading
import unittest
import urllib.error
import urllib.request

import kit

from whizbang_sets import egress, generators

TOKEN = "made-up-token-for-a-test"


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _answer(self):
        self.server.seen.append((self.command, self.path, self.headers.get("Authorization")))
        if self.command == "POST":
            self.rfile.read(int(self.headers.get("Content-Length") or 0))
        to = self.server.redirect.get(self.path)
        if to:
            self.send_response(302)
            self.send_header("Location", to)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        body = self.server.body
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = do_POST = _answer


class Server:
    """One server on 127.0.0.1, in one daemon thread, shut down at the end."""

    def __init__(self, body=b"{}"):
        self.httpd = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.httpd.seen, self.httpd.redirect, self.httpd.body = [], {}, body
        self.origin = "http://127.0.0.1:%d" % self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, args=(0.05,), daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(5)
        return False

    @property
    def seen(self):
        return self.httpd.seen


def approved(*recipients):
    return egress.Approved(
        [
            egress.Recipient(name=f"r{n}", origin=o, upstreams=frozenset(u), classes=frozenset(c))
            for n, (o, u, c) in enumerate(recipients)
        ]
    )


def sender(a):
    return egress.Sender(a, sleep=lambda s: None)


class Origins(unittest.TestCase):
    def test_an_origin_is_its_scheme_host_and_port_as_the_config_spells_them(self):
        self.assertEqual(egress.origin_of("HTTPS://Fal.Run:443/x?y"), "https://fal.run")
        self.assertEqual(egress.origin_of("http://127.0.0.1:8080/a"), "http://127.0.0.1:8080")
        with_user = "https://someone" + chr(64) + "h.example/"  # an address naming a user
        for bad in ("file:///etc/x", with_user, "ftp://h/", "https:///x", "https://h:port/"):
            with self.assertRaises(egress.Refused, msg=bad):
                egress.origin_of(bad)

    def test_the_config_is_read_for_its_recipients(self):
        with kit.Scratch() as s:
            path = os.path.join(s, "whizbang.toml")
            with open(path, "w") as f:
                f.write(
                    "[spend]\ncall_cap_usd = 1\n\n[egress.recipients.fal]\n"
                    'origin = "https://FAL.run"\nupstreams = ["https://cdn.fal.example"]\n'
                    'classes = ["prompts", "source_images"]\n'
                )
            a = egress.Approved.from_config(path)
            r = a.recipient_for("https://fal.run/x", ("prompts",))
            self.assertTrue(r.reaches("https://cdn.fal.example"))
            with self.assertRaises(egress.Refused):
                a.recipient_for("https://fal.run/x", ("prompts", "derived_images"))
            with open(path, "a") as f:
                f.write('\n[egress.recipients.other]\norigin = "https://o.example"\nclasses = ["secrets"]\n')
            with self.assertRaisesRegex(ValueError, "unknown class"):
                egress.Approved.from_config(path)


class Checked(unittest.TestCase):
    def test_a_request_to_an_origin_the_config_does_not_approve_is_refused_before_it_is_sent(self):
        with Server() as there:
            s = sender(approved(("https://fal.run", (), ("prompts",))))
            with self.assertRaisesRegex(egress.Refused, "nothing was sent"):
                s.post(there.origin + "/x", b"{}", {}, {"Authorization": TOKEN}, ("prompts",), "x")
            # Approved, but not for a picture: refused too.
            s = sender(approved((there.origin, (), ("prompts",))))
            with self.assertRaises(egress.Refused):
                s.post(there.origin + "/x", b"{}", {}, {}, ("prompts", "source_images"), "x")
            self.assertEqual(there.seen, [])

    def test_a_redirect_to_an_origin_the_config_does_not_approve_is_refused(self):
        with Server() as there, Server() as elsewhere:
            there.httpd.redirect["/x"] = elsewhere.origin + "/y"
            s = sender(approved((there.origin, (), ("prompts",))))
            with self.assertRaisesRegex(egress.Refused, "redirect"):
                s.post(there.origin + "/x", b"{}", {}, {"Authorization": TOKEN}, ("prompts",), "x")
            self.assertEqual(len(there.seen), 1)
            self.assertEqual(elsewhere.seen, [], "nothing reached the origin that is not approved")

    def test_a_redirect_to_an_upstream_is_followed_and_the_key_stays_behind(self):
        with Server() as there, Server(body=b"picture") as cdn:
            there.httpd.redirect["/x"] = cdn.origin + "/y"
            s = sender(approved((there.origin, (cdn.origin,), ("prompts",))))
            answer, _ = s.post(there.origin + "/x", b"{}", {}, {"Authorization": TOKEN}, ("prompts",), "x")
            self.assertEqual(answer, b"picture")
            self.assertEqual(there.seen[0][2], TOKEN)
            self.assertEqual(cdn.seen, [("GET", "/y", None)])

    def test_a_download_from_an_origin_the_config_does_not_approve_is_refused(self):
        with Server() as there, Server(body=b"picture") as cdn, Server() as elsewhere:
            a = approved((there.origin, (cdn.origin,), ("prompts",)))
            via = a.recipients[0]
            s = sender(a)
            self.assertEqual(s.get(cdn.origin + "/p.png", via, "x"), b"picture")
            self.assertEqual(cdn.seen, [("GET", "/p.png", None)])
            with self.assertRaisesRegex(egress.Refused, "download"):
                s.get(elsewhere.origin + "/p.png", via, "x")
            cdn.httpd.redirect["/moved"] = elsewhere.origin + "/p.png"
            with self.assertRaisesRegex(egress.Refused, "redirect"):
                s.get(cdn.origin + "/moved", via, "x")
            self.assertEqual(elsewhere.seen, [])

    def test_no_proxy_is_used_whatever_the_environment_names(self):
        made = []

        def opener(*handlers):
            made.extend(handlers)
            return scripted([b"ok"])[0]()

        os.environ["https_proxy"] = "http://127.0.0.1:9"
        try:
            s = egress.Sender(approved(("https://x.example", (), ("prompts",))), opener=opener)
            self.assertEqual(s.post("https://x.example/a", b"", {}, {}, ("prompts",), "x")[0], b"ok")
        finally:
            del os.environ["https_proxy"]
        proxies = [h for h in made if isinstance(h, urllib.request.ProxyHandler)]
        self.assertEqual([p.proxies for p in proxies], [{}])


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False


def scripted(outcomes):
    """An opener whose `open` answers with each outcome in turn, and the requests it saw."""
    calls = []

    class Opener:
        def open(self, req, timeout):
            calls.append(req)
            o = outcomes[len(calls) - 1]
            if isinstance(o, Exception):
                raise o
            return Response(o)

    return (lambda *handlers: Opener()), calls


def http_error(code):
    return urllib.error.HTTPError("https://x", code, "err", {}, io.BytesIO(b'{"error": "x"}'))


class Retries(unittest.TestCase):
    def a(self):
        return approved(("https://x.example", (), ("prompts",)))

    def test_a_passing_failure_is_tried_again_after_each_wait(self):
        opener, calls = scripted([http_error(503), urllib.error.URLError("reset"), b"ok"])
        waits = []
        s = egress.Sender(self.a(), sleep=waits.append, opener=opener)
        self.assertEqual(s.post("https://x.example/a", b"", {}, {}, ("prompts",), "x")[0], b"ok")
        self.assertEqual((len(calls), waits), (3, list(egress.RETRY_WAITS[:2])))

    def test_a_client_error_is_not_tried_again(self):
        opener, calls = scripted([http_error(401)])
        s = egress.Sender(self.a(), sleep=lambda w: None, opener=opener)
        with self.assertRaisesRegex(egress.GeneratorError, "HTTP 401"):
            s.post("https://x.example/a", b"", {}, {}, ("prompts",), "x")
        self.assertEqual(len(calls), 1)

    def test_a_lasting_outage_gives_up_after_every_wait(self):
        n = len(egress.RETRY_WAITS) + 1
        opener, calls = scripted([http_error(503)] * n)
        s = egress.Sender(self.a(), sleep=lambda w: None, opener=opener)
        with self.assertRaisesRegex(egress.GeneratorError, "HTTP 503"):
            s.post("https://x.example/a", b"", {}, {}, ("prompts",), "x")
        self.assertEqual(len(calls), n)


class Recording:
    """A sender that answers each POST with a set body and records what each carried."""

    def __init__(self, answer, download=b""):
        self.answer, self.download = answer, download
        self.posts, self.gets = [], []

    def post(self, url, body, headers, credentials, classes, what):
        self.posts.append((url, classes, credentials))
        return json.dumps(self.answer).encode(), "recipient"

    def get(self, url, via, what):
        self.gets.append((url, via))
        return self.download


class Generators(unittest.TestCase):
    def setUp(self):
        for k in ("FAL_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"):
            os.environ[k] = TOKEN

    def tearDown(self):
        for k in ("FAL_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY"):
            os.environ.pop(k, None)

    def test_fal_sends_a_prompt_alone_to_generate_and_a_picture_with_it_to_edit(self):
        picture = b"\x89PNG\r\n\x1a\nfake"
        r = Recording({"images": [{"url": "data:image/png;base64," + base64.b64encode(picture).decode()}]})
        fal = generators.FalKlein(r)
        self.assertEqual(fal.generate("a lake", 1, 64), picture)
        self.assertEqual(fal.edit(picture, "no boat", 2), picture)
        self.assertEqual([p[1] for p in r.posts], [("prompts",), ("prompts", "source_images")])
        self.assertTrue(all(p[0].startswith("https://fal.run/") for p in r.posts))
        self.assertEqual(r.gets, [], "a picture in the answer is not downloaded")

    def test_fal_downloads_a_picture_it_answers_with_an_address_through_the_check(self):
        r = Recording({"images": [{"url": "https://cdn.fal.example/p.png"}]}, download=b"pic")
        self.assertEqual(generators.FalKlein(r).generate("a lake", 1, 64), b"pic")
        self.assertEqual(r.gets, [("https://cdn.fal.example/p.png", "recipient")])

    def test_openai_and_gemini_read_the_picture_from_the_answer(self):
        data = base64.b64encode(b"pic").decode()
        r = Recording({"data": [{"b64_json": data}]})
        openai = generators.OpenAiImage(r)
        self.assertEqual(openai.generate("a lake", 1, 64), b"pic")
        self.assertEqual(openai.edit(b"\x89PNG\r\n\x1a\n", "no boat", 1), b"pic")
        self.assertEqual([p[1] for p in r.posts], [("prompts",), ("prompts", "source_images")])
        r = Recording({"candidates": [{"content": {"parts": [{"inlineData": {"data": data}}]}}]})
        self.assertEqual(generators.GeminiImage(r).edit(b"\xff\xd8", "no boat", 1), b"pic")
        self.assertEqual(r.posts[0][1], ("prompts", "source_images"))
        with self.assertRaises(egress.GeneratorError):
            generators.GeminiImage(Recording({"candidates": []})).generate("x", 1, 64)

    def test_a_missing_key_is_said_by_its_name(self):
        del os.environ["FAL_KEY"]
        with self.assertRaisesRegex(egress.GeneratorError, "FAL_KEY is not set"):
            generators.FalKlein(Recording({})).generate("x", 1, 64)


if __name__ == "__main__":
    unittest.main()
