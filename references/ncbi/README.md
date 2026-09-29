# NCBI genetic codes

`gc.prt` is the NCBI genetic code table, version 4.6, downloaded 2026-09-28 from
<https://ftp.ncbi.nlm.nih.gov/entrez/misc/data/gc.prt> (SHA-256
`2aecbdd09ecc5e0d233c105090a593737298ef93d88f6bc0c1bef26df6051a04`). NCBI data are
in the public domain. `python3 scripts/generate_genetic_codes.py` checks the hash and
regenerates `crates/dnagent-domain/src/genetic_codes.rs`; never edit that file by hand.
