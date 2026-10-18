"""Request A preflight (Issue #18): atomic NO-CLOBBER publication in tools/fetch_pinned.py.

Loopback fixtures only. Nothing here contacts a remote host, and nothing downloads, opens or runs jq or any other third-party program.
The tests run unchanged on Windows and macOS (CI), which is what proves the hard-link publication primitive on both systems.
"""

import errno
import hashlib
import http.server
import importlib.util
import os
import pathlib
import socketserver
import stat
import tempfile
import threading
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


fp = load("fetch_pinned")

BODY = bytes(range(256)) * 40
SHA = hashlib.sha256(BODY).hexdigest()


class Handler(http.server.BaseHTTPRequestHandler):
    port = 0
    hits = 0

    def log_message(self, *a):
        pass

    def do_GET(self):
        Handler.hits += 1
        path = self.path.split("?")[0]
        self.send_response(200)
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        if path == "/ok":
            self.wfile.write(BODY)
        elif path == "/cut":
            # promises the full length, sends half and drops the connection: a transfer interruption
            self.wfile.write(BODY[: len(BODY) // 2])
            self.wfile.flush()
            self.close_connection = True
        else:
            self.wfile.write(BODY)


def symlink_or_skip(test, target, link, **kw):
    try:
        os.symlink(target, link, **kw)
    except (OSError, NotImplementedError) as e:
        test.skipTest(f"cannot create symlinks here: {e}")


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Handler)
        cls.server.daemon_threads = True
        Handler.port = cls.server.server_address[1]
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        Handler.hits = 0
        self.tmp = tempfile.mkdtemp()
        self.dest = os.path.join(self.tmp, "artifact.bin")
        self.addCleanup(setattr, fp, "_before_publish", None)

    def fetch(self, path="/ok", size=len(BODY), sha=SHA, dest=None):
        return fp.fetch(f"http://127.0.0.1:{Handler.port}{path}", dest or self.dest, size, sha, ["127.0.0.1"], loopback_http=True)

    def refused(self, path="/ok", **kw):
        with self.assertRaises(fp.Refused) as cm:
            self.fetch(path, **kw)
        return str(cm.exception)

    def listing(self):
        return sorted(os.listdir(self.tmp))


class Publication(Base):
    def test_success_leaves_exactly_the_one_verified_private_file(self):
        rec = self.fetch()
        self.assertEqual((rec["size"], rec["sha256"]), (len(BODY), SHA))
        self.assertNotIn("leftovers", rec)
        self.assertEqual(self.listing(), ["artifact.bin"], "no staging file, no probe file")
        with open(self.dest, "rb") as fh:
            self.assertEqual(fh.read(), BODY)
        st = os.lstat(self.dest)
        self.assertTrue(stat.S_ISREG(st.st_mode))
        self.assertEqual(st.st_nlink, 1, "the staging name is gone")
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE(st.st_mode) & 0o177, 0, "owner-only, no execute bit")

    def test_the_source_has_no_replace_rename_copy_or_check_then_publish(self):
        text = (ROOT / "tools" / "fetch_pinned.py").read_text(encoding="utf-8")
        for needle in ["os.replace", "os.rename", "shutil", "copyfile", "copy2", ".part\"", "os.path.exists(dest)", "os.link("]:
            if needle == "os.link(":
                self.assertIn(needle, text)  # the one primitive
            else:
                self.assertNotIn(needle, text, needle)
        self.assertEqual(text.count("os.link("), 2, "only the probe/publish helper uses it (follow_symlinks=False / default branch)")


class PreExistingDestination(Base):
    def assert_untouched_and_nothing_downloaded(self, expect_listing):
        self.assertEqual(Handler.hits, 0, "refused before any network use")
        self.assertEqual(self.listing(), expect_listing)

    def test_an_existing_file_is_refused_and_kept(self):
        with open(self.dest, "wb") as fh:
            fh.write(b"mine")
        self.assertEqual(self.refused(), "destination_exists")
        with open(self.dest, "rb") as fh:
            self.assertEqual(fh.read(), b"mine")
        self.assert_untouched_and_nothing_downloaded(["artifact.bin"])

    def test_an_existing_directory_is_refused_and_kept(self):
        os.mkdir(self.dest)
        open(os.path.join(self.dest, "inner"), "wb").close()
        self.assertEqual(self.refused(), "destination_exists")
        self.assertEqual(os.listdir(self.dest), ["inner"])
        self.assert_untouched_and_nothing_downloaded(["artifact.bin"])

    def test_a_symlink_to_an_existing_file_is_refused_and_its_target_is_kept(self):
        target = os.path.join(self.tmp, "target.bin")
        with open(target, "wb") as fh:
            fh.write(b"precious")
        symlink_or_skip(self, target, self.dest)
        self.assertEqual(self.refused(), "destination_exists")
        self.assertTrue(os.path.islink(self.dest))
        with open(target, "rb") as fh:
            self.assertEqual(fh.read(), b"precious")
        self.assert_untouched_and_nothing_downloaded(["artifact.bin", "target.bin"])

    def test_a_dangling_symlink_is_refused_and_not_followed(self):
        target = os.path.join(self.tmp, "does-not-exist.bin")
        symlink_or_skip(self, target, self.dest)
        self.assertFalse(os.path.exists(self.dest), "it really dangles")
        self.assertEqual(self.refused(), "destination_exists")
        self.assertTrue(os.path.islink(self.dest))
        self.assertFalse(os.path.lexists(target), "nothing was created through the link")
        self.assert_untouched_and_nothing_downloaded(["artifact.bin"])

    def test_the_destination_folder_must_be_a_real_directory(self):
        real = os.path.join(self.tmp, "real")
        os.mkdir(real)
        link = os.path.join(self.tmp, "link")
        symlink_or_skip(self, real, link, target_is_directory=True)
        self.assertEqual(self.refused(dest=os.path.join(link, "artifact.bin")), "destination_folder_not_a_directory")
        self.assertEqual(os.listdir(real), [])
        self.assertEqual(self.refused(dest=os.path.join(self.tmp, "no-such-folder", "artifact.bin")), "destination_folder_missing")
        self.assertEqual(Handler.hits, 0)

    def test_unrelated_and_old_style_part_files_are_never_touched(self):
        for name, data in [("artifact.bin.part", b"old part"), ("unrelated.txt", b"unrelated")]:
            with open(os.path.join(self.tmp, name), "wb") as fh:
                fh.write(data)
        rec = self.fetch()
        self.assertEqual(rec["sha256"], SHA)
        self.assertEqual(self.listing(), ["artifact.bin", "artifact.bin.part", "unrelated.txt"])
        for name, data in [("artifact.bin.part", b"old part"), ("unrelated.txt", b"unrelated")]:
            with open(os.path.join(self.tmp, name), "rb") as fh:
                self.assertEqual(fh.read(), data)


class DestinationAppearsAtThePublicationBoundary(Base):
    """The entry is created after the download is fully verified and before the publication step: the old check-then-replace lost here."""

    def run_case(self, make):
        def hook(part, dest):
            make(dest)
        fp._before_publish = hook
        self.assertEqual(self.refused(), "destination_exists")
        fp._before_publish = None
        self.assertEqual(Handler.hits, 1, "the download did happen; only publication was refused")

    def test_a_file_created_at_the_boundary_is_not_overwritten(self):
        def make(dest):
            with open(dest, "wb") as fh:
                fh.write(b"someone else")
        self.run_case(make)
        with open(self.dest, "rb") as fh:
            self.assertEqual(fh.read(), b"someone else")
        self.assertEqual(self.listing(), ["artifact.bin"], "our staging file was removed, theirs kept")

    def test_a_directory_created_at_the_boundary_is_not_replaced(self):
        def make(dest):
            os.mkdir(dest)
            open(os.path.join(dest, "inner"), "wb").close()
        self.run_case(make)
        self.assertEqual(os.listdir(self.dest), ["inner"])
        self.assertEqual(self.listing(), ["artifact.bin"])

    def test_a_dangling_symlink_created_at_the_boundary_is_not_replaced_or_followed(self):
        target = os.path.join(self.tmp, "nowhere.bin")

        def make(dest):
            symlink_or_skip(self, target, dest)
        self.run_case(make)
        self.assertTrue(os.path.islink(self.dest))
        self.assertFalse(os.path.lexists(target))
        self.assertEqual(self.listing(), ["artifact.bin"])

    def test_a_symlink_to_a_file_created_at_the_boundary_leaves_the_target_alone(self):
        target = os.path.join(self.tmp, "target.bin")
        with open(target, "wb") as fh:
            fh.write(b"precious")

        def make(dest):
            symlink_or_skip(self, target, dest)
        self.run_case(make)
        with open(target, "rb") as fh:
            self.assertEqual(fh.read(), b"precious")
        self.assertEqual(self.listing(), ["artifact.bin", "target.bin"])

    def test_many_simultaneous_publishers_produce_exactly_one_winner_and_no_overwrite(self):
        results = []
        barrier = threading.Barrier(6)
        lock = threading.Lock()

        def gate(part, dest):
            barrier.wait(timeout=20)  # all six have verified bytes and stand at the boundary together

        fp._before_publish = gate

        def worker():
            try:
                r = self.fetch()
                out = ("ok", r["sha256"])
            except fp.Refused as e:
                out = ("refused", str(e))
            except Exception as e:  # a broken barrier must not hide as a pass
                out = ("error", repr(e))
            with lock:
                results.append(out)

        threads = [threading.Thread(target=worker) for _ in range(6)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(60)
        fp._before_publish = None
        self.assertEqual(len(results), 6, results)
        self.assertEqual(sum(1 for r in results if r[0] == "ok"), 1, results)
        self.assertEqual([r for r in results if r[0] != "ok"], [("refused", "destination_exists")] * 5)
        with open(self.dest, "rb") as fh:
            self.assertEqual(fh.read(), BODY)
        self.assertEqual(self.listing(), ["artifact.bin"], "every loser removed only its own staging file")

    def test_an_interrupt_at_the_boundary_keeps_no_destination_and_no_staging_file(self):
        def hook(part, dest):
            raise KeyboardInterrupt()
        fp._before_publish = hook
        with self.assertRaises(KeyboardInterrupt):
            self.fetch()
        fp._before_publish = None
        self.assertEqual(self.listing(), [])


class TransferProblems(Base):
    def test_an_interrupted_transfer_publishes_nothing_and_cleans_up(self):
        reason = self.refused("/cut")
        self.assertTrue(reason.startswith(("transport:", "size_differs")), reason)
        self.assertEqual(self.listing(), [])

    def test_wrong_size_and_wrong_digest_publish_nothing_and_clean_up(self):
        self.assertTrue(self.refused(size=len(BODY) + 1).startswith("content_length_differs"))
        self.assertEqual(self.refused(sha="0" * 64), "sha256_differs")
        self.assertEqual(self.listing(), [])
        self.assertFalse(os.path.lexists(self.dest))


class UnsupportedFilesystem(Base):
    def no_fallback(self):
        boom = AssertionError("a fallback publication primitive was used")
        return (mock.patch.object(fp.os, "replace", side_effect=boom), mock.patch.object(fp.os, "rename", side_effect=boom))

    def test_a_folder_that_cannot_hard_link_is_refused_before_any_download(self):
        a, b = self.no_fallback()
        with a, b, mock.patch.object(fp.os, "link", side_effect=OSError(errno.EPERM, "not permitted")):
            reason = self.refused()
        self.assertTrue(reason.startswith("no_clobber_publication_unsupported"), reason)
        self.assertEqual(Handler.hits, 0)
        self.assertEqual(self.listing(), [])
        self.assertFalse(os.path.lexists(self.dest))

    def test_a_filesystem_whose_link_silently_overwrites_or_ignores_an_existing_name_is_refused(self):
        real = os.link
        calls = []

        def lax(src, dst, *args, **kw):
            calls.append(dst)
            if len(calls) == 1:
                return real(src, dst, *args, **kw)
            return None  # second link onto an existing name "succeeds": broken no-clobber semantics

        a, b = self.no_fallback()
        with a, b, mock.patch.object(fp.os, "link", side_effect=lax):
            reason = self.refused()
        self.assertEqual(reason, "no_clobber_publication_unsupported:existing_entry_overwritten_or_ignored")
        self.assertEqual(Handler.hits, 0)
        self.assertEqual(self.listing(), [])

    def test_a_link_that_fails_only_at_publication_refuses_without_fallback_and_cleans_staging(self):
        real = os.link

        def selective(src, dst, *args, **kw):
            if os.path.basename(dst).startswith(".atm-probe"):
                return real(src, dst, *args, **kw)
            raise OSError(errno.EPERM, "not permitted")

        a, b = self.no_fallback()
        with a, b, mock.patch.object(fp.os, "link", side_effect=selective):
            reason = self.refused()
        self.assertEqual(reason, f"no_clobber_publication_unsupported:{errno.EPERM}")
        self.assertEqual(Handler.hits, 1)
        self.assertEqual(self.listing(), [])
        self.assertFalse(os.path.lexists(self.dest))


class SafeCleanup(Base):
    def test_a_staging_name_that_now_holds_someone_elses_file_is_preserved(self):
        def hook(part, dest):
            os.remove(part)
            with open(part, "wb") as fh:
                fh.write(b"not ours")

        fp._before_publish = hook
        self.assertEqual(self.refused(), "staged_file_replaced")
        fp._before_publish = None
        leftovers = [n for n in self.listing() if n.startswith("artifact.bin.part-")]
        self.assertEqual(len(leftovers), 1)
        with open(os.path.join(self.tmp, leftovers[0]), "rb") as fh:
            self.assertEqual(fh.read(), b"not ours")
        self.assertFalse(os.path.lexists(self.dest))

    def test_when_the_staging_name_cannot_be_removed_it_is_kept_and_reported_and_the_file_is_still_published(self):
        with mock.patch.object(fp.os, "unlink", side_effect=PermissionError(errno.EACCES, "locked")):
            rec = self.fetch()
        self.assertEqual(len(rec["leftovers"]), 1)
        self.assertTrue(rec["leftovers"][0].startswith("artifact.bin.part-"))
        with open(os.path.join(self.tmp, rec["leftovers"][0]), "rb") as fh:
            self.assertEqual(fh.read(), BODY)
        with open(self.dest, "rb") as fh:
            self.assertEqual(fh.read(), BODY)

    def test_the_hard_link_primitive_itself_never_replaces_any_kind_of_existing_entry_on_this_operating_system(self):
        src = os.path.join(self.tmp, "src.bin")
        with open(src, "wb") as fh:
            fh.write(b"new")
        entries = {}
        f = os.path.join(self.tmp, "file")
        with open(f, "wb") as fh:
            fh.write(b"old")
        entries["file"] = f
        d = os.path.join(self.tmp, "dir")
        os.mkdir(d)
        entries["dir"] = d
        s = os.path.join(self.tmp, "dangling")
        try:
            os.symlink(os.path.join(self.tmp, "nowhere"), s)
            entries["dangling symlink"] = s
        except (OSError, NotImplementedError):
            pass
        for kind, path in entries.items():
            before = os.lstat(path)
            with self.assertRaises(OSError, msg=kind) as cm:
                fp._link_no_clobber(src, path)
            if not isinstance(cm.exception, FileExistsError):
                # some systems report a directory differently; the contract is "refused and still there"
                self.assertTrue(os.path.lexists(path), kind)
            after = os.lstat(path)
            self.assertEqual((before.st_mode, before.st_ino, before.st_size), (after.st_mode, after.st_ino, after.st_size), kind)
        with open(f, "rb") as fh:
            self.assertEqual(fh.read(), b"old")
        with open(src, "rb") as fh:
            self.assertEqual(fh.read(), b"new")


if __name__ == "__main__":
    unittest.main()
