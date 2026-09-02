codeintel
=========

Official compiled release of codeintel.

codeintel indexes Git repositories into a bindb store as a semantic graph.
This archive contains the public executable only. Source is privately maintained.

Installation
------------

1. Verify the archive against checksums from the GitHub Release.
2. Extract this archive.
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
