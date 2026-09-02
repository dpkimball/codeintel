codeintel
=========

Official compiled release of codeintel.

codeintel indexes Git repositories into a bindb store as a semantic graph.
This archive contains the public executable only. Source is privately maintained.

Installation
------------

1. Verify the archive against the matching `*.tar.xz.sha256` file from the
   GitHub Release (or use the release’s `sha256.sum`).
2. Extract this archive (`tar -xJf …` for `.tar.xz`).
3. Place `codeintel` on your PATH.

Verify
------

    codeintel --version
    codeintel --help

Doctrine
--------

Default doctrine is portable (no product crate preload). Binder Desktop
layering is opt-in: CODEINTEL_DOCTRINE_PROFILE=binder or --profile binder.

License
-------

See LICENSE.txt. Proprietary binary distribution — not open-source software.

Support
-------

See https://github.com/dpkimball/codeintel-releases
