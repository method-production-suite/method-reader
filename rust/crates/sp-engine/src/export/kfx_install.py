
import bz2, io, json, os, tempfile, urllib.request, zipfile

# Raised only around the two NETWORK fetches below, so the outer except
# clauses can tell "could not reach the index or the zip" (most likely:
# offline) apart from every other way this can fail, and report the
# first one as one plain sentence instead of raw urllib/http text. Defined
# here, before the try, and not as the first line inside it: if anything
# raised before that line would have run, 'except Unreachable' would be
# evaluated against a name that was never bound, and the failure would be
# a bare NameError with no METHOD_READER_RESULT line at all.
class Unreachable(Exception):
    pass

# Older copies cleared to make room. Bound here, beside Unreachable and for
# the same reason, so BOTH failure lines below can report it: add_plugin
# failing after the clearing loop has already taken a copy out, and the
# caller must be able to say which.
removed = []

try:
    from calibre.utils.https import get_https_resource_securely
    from calibre.gui2.dialogs.plugin_updater import INDEX_URL
    from calibre.customize.ui import (
        add_plugin, initialized_plugins, output_format_plugins, remove_plugin)
    try:
        idx_raw = get_https_resource_securely(INDEX_URL)
    except Exception as e:
        raise Unreachable(str(e))
    idx = json.loads(bz2.decompress(idx_raw).decode('utf-8'))
    meta = idx.get('KFX Output')
    if meta is None:
        raise RuntimeError('KFX Output is not in calibre\'s plugin index')
    # Built OUTSIDE the try below: an index entry with no 'file' is a broken
    # index, not a lost connection, and must not be reported as one.
    zip_url = 'https://plugins.calibre-ebook.com/' + meta['file']
    try:
        data = urllib.request.urlopen(zip_url, timeout=120).read()
    except Exception as e:
        raise Unreachable(str(e))
    if len(data) != meta['size']:
        raise RuntimeError('downloaded %d bytes, index says %d' % (len(data), meta['size']))
    if '__init__.py' not in zipfile.ZipFile(io.BytesIO(data)).namelist():
        raise RuntimeError('downloaded file is not a calibre plugin')
    # Clear FORKS first. A variant registers the same internal package
    # (calibre_plugins.kfx_output) under a different plugin NAME, so calibre
    # happily holds both and the new plugin's code then imports the fork's
    # kfxlib -- KFX conversion dies with ImportError and the toolchain still
    # reports ready. Adding without clearing is worse than not installing.
    #
    # Only CONVERSION OUTPUT plugins can collide for the .kfx slot, and
    # calibre is asked which those are rather than guessed at by name. A
    # name-only test also matched "Set KFX metadata (from KFX Output)",
    # which is the companion metadata writer shipping in the same zip, and
    # deleted it; it survived only because add_plugin put it back.
    #
    # It runs after the download and its checks, so every failure up to this
    # point, offline included, leaves whatever was there untouched. From
    # here on it no longer holds: if add_plugin fails below, a copy may
    # already be gone, which is why both failure lines report 'removed'.
    outs = set()
    for p in output_format_plugins():
        outs.add(getattr(p, 'name', ''))
    for p in list(initialized_plugins()):
        n = getattr(p, 'name', '')
        if n in outs and 'KFX Output' in n and n != 'KFX Output':
            remove_plugin(p)
            removed.append(n)
    fd, path = tempfile.mkstemp(suffix='.zip')
    try:
        os.write(fd, data); os.close(fd)
        add_plugin(path)
    finally:
        os.unlink(path)
    print('METHOD_READER_RESULT ' + json.dumps(
        {'ok': True, 'version': '.'.join(map(str, meta['version'])), 'removed': removed}))
except Unreachable as e:
    print('METHOD_READER_RESULT ' + json.dumps(
        {'ok': False, 'error': str(e), 'offline': True, 'removed': removed}))
except Exception as e:
    print('METHOD_READER_RESULT ' + json.dumps({'ok': False, 'error': str(e), 'removed': removed}))
