//! Tiny deterministic media fixtures shared by the protocol examples and
//! conformance resources. They are generated data embedded in the binary;
//! no ArtCraft API traffic or generation credits are involved.

/// A valid 1x1 red RGB PNG (69 bytes), base64 encoded.
pub const PNG_1X1_RED_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

/// A valid 50 ms 8 kHz mono 16-bit WAV containing a quiet 440 Hz tone
/// (844 bytes), base64 encoded.
pub const WAV_440HZ_BASE64: &str =
    "UklGRkQDAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YSADAAAAAEoF9QlyDVkPbg+xDVUKwAV9ACz7bfbP8sHwgPAU8k71zPkF/1sELwnsDCIPjg8jDgwLpwZ4AR78OPdc8//waPCp8Z306fgL/mgDXwhYDNwOng+HDrgLhwdxAhP9DPj2803xYPBN8fbzDPgT/XEChwe4C4cOng/cDlgMXwhoAwv+6fid9KnxaPD/8FzzOPce/HgBpwYMCyMOjg8iD+wMLwlbBAX/zPlO9RTygPDB8M/ybfYs+30AwAVVCrENbg9ZD3IN9QlKBQAAtvoL9o7yp/CS8E/yq/VA+oP/1ASTCTENPw+AD+wNsgo0BvsApfvR9hTz3vBy8N3x9PRZ+Yj+4gPICKQMAQ+YD1cOYwsXB/UBmPyh96jzJPFi8HnxSPR5+I/97QL0BwoMsw6gD7MOCgz0B+0Cj/15+Ej0efFi8CTxqPOh95j89QEXB2MLVw6YDwEPpAzICOIDiP5Z+fT03fFy8N7wFPPR9qX7+wA0BrIK7A2ADz8PMQ2TCdQEg/9A+qv1T/KS8KfwjvIL9rb6AABKBfUJcg1ZD24PsQ1VCsAFfQAs+232z/LB8IDwFPJO9cz5Bf9bBC8J7AwiD44PIw4MC6cGeAEe/Dj3XPP/8GjwqfGd9On4C/5oA18IWAzcDp4Phw64C4cHcQIT/Qz49vNN8WDwTfH28wz4E/1xAocHuAuHDp4P3A5YDF8IaAML/un4nfSp8Wjw//Bc8zj3Hvx4AacGDAsjDo4PIg/sDC8JWwQF/8z5TvUU8oDwwfDP8m32LPt9AMAFVQqxDW4PWQ9yDfUJSgUAALb6C/aO8qfwkvBP8qv1QPqD/9QEkwkxDT8PgA/sDbIKNAb7AKX70fYU897wcvDd8fT0WfmI/uIDyAikDAEPmA9XDmMLFwf1AZj8ofeo8yTxYvB58Uj0efiP/e0C9AcKDLMOoA+zDgoM9AftAo/9efhI9HnxYvAk8ajzofeY/PUBFwdjC1cOmA8BD6QMyAjiA4j+Wfn09N3xcvDe8BTz0fal+/sANAayCuwNgA8/DzENkwnUBIP/QPqr9U/ykvCn8I7yC/a2+g==";
