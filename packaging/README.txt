codeintel
=========

Official compiled release of codeintel.

Indexes Git repositories into a bindb store as a semantic graph.
Public executable only — source is private.

There are no SQL tables. A store is a directory (commits.wal).
Use YOUR store path and YOUR git checkouts — nothing DPK-specific required.

Prerequisites
-------------

Public bindb v0.1.2+ from https://github.com/dpkimball/bindb-releases
  (bindb init creates the store)

Installation
------------

1. Verify `*.tar.xz.sha256` from the GitHub Release.
2. Extract (`tar -xJf …`).
3. Place `codeintel` on your PATH.

Quick start (your repos, your store)
------------------------------------

    bindb init ./mystore
    codeintel index /path/to/repo --store ./mystore --name my-repo
    codeintel query --store ./mystore --name my-repo --question summary
    codeintel doctrine --store ./mystore --name my-repo --profile auto

Full guide:
https://github.com/dpkimball/codeintel-releases/blob/master/GETTING_STARTED.md

Doctrine
--------

Default is portable (--profile auto). Binder Desktop ranks are opt-in:
CODEINTEL_DOCTRINE_PROFILE=binder or --profile binder.

Verify
------

    codeintel --version
    codeintel --help

License
-------

See LICENSE.txt. Proprietary binary distribution — not open-source software.

Support
-------

https://github.com/dpkimball/codeintel-releases
