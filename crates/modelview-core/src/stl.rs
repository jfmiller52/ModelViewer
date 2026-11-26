use crate::mesh::Mesh;
use anyhow::{Context, Result, bail};
use rayon::prelude::*;

/// Parse either flavour of STL.
pub fn parse(bytes: &[u8]) -> Result<Mesh> {
    if looks_like_ascii(bytes) {
        match parse_ascii(bytes) {
            Ok(mesh) => Ok(mesh),
            Err(ascii_error) => parse_binary(bytes)
                .with_context(|| format!("not valid ASCII STL ({ascii_error}) or binary STL")),
        }
    } else {
        parse_binary(bytes)
    }
}

fn looks_like_ascii(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    head.starts_with(b"solid") && head.windows(5).any(|w| w.eq_ignore_ascii_case(b"facet"))
}

fn parse_binary(bytes: &[u8]) -> Result<Mesh> {
    if bytes.len() < 84 {
        bail!("file is {} bytes, too short for an STL header", bytes.len());
    }
    let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if count == 0 {
        return Ok(Mesh::default());
    }
    if count > 100_000_000 {
        bail!("header claims {count} triangles, which is not plausible");
    }
    let expected = 84usize
        .checked_add(count.checked_mul(50).context("triangle count overflow")?)
        .context("triangle count overflow")?;
    if expected > bytes.len() {
        bail!(
            "truncated binary STL: header claims {count} triangles ({} bytes) but the file is {} bytes",
            expected,
            bytes.len()
        );
    }
    let records = &bytes[84..expected];
    let mut positions = vec![[0.0f32; 3]; count * 3];
    positions
        .par_chunks_mut(3)
        .zip(records.par_chunks_exact(50))
        .for_each(|(out, rec)| {
            for (i, slot) in out.iter_mut().enumerate() {
                let o = 12 + i * 12;
                *slot = [read_f32(rec, o), read_f32(rec, o + 4), read_f32(rec, o + 8)];
                for c in slot.iter_mut() {
                    if !c.is_finite() {
                        *c = 0.0;
                    }
                }
            }
        });
    let indices = (0..count as u32)
        .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
        .collect();
    Ok(Mesh { positions, indices })
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn parse_ascii(bytes: &[u8]) -> Result<Mesh> {
    let text = std::str::from_utf8(bytes).context("ASCII STL is not valid UTF-8")?;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if words.next() == Some("vertex") {
            let x = words.next().and_then(|w| w.parse::<f32>().ok());
            let y = words.next().and_then(|w| w.parse::<f32>().ok());
            let z = words.next().and_then(|w| w.parse::<f32>().ok());
            match (x, y, z) {
                (Some(x), Some(y), Some(z)) => positions.push([x, y, z]),
                _ => bail!("bad vertex line: {line}"),
            }
        }
    }
    if positions.is_empty() {
        bail!("ASCII STL contains no vertices");
    }
    if !positions.len().is_multiple_of(3) {
        bail!(
            "ASCII STL has {} vertices, which is not a whole number of triangles",
            positions.len()
        );
    }
    let indices = (0..positions.len() as u32 / 3)
        .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
        .collect();
    Ok(Mesh { positions, indices })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_binary() -> Vec<u8> {
        // 12 triangles, unit cube, in millimetres.
        let mut out = vec![0u8; 80];
        out.extend_from_slice(&12u32.to_le_bytes());
        let faces: [[[f32; 3]; 3]; 12] = [
            [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
            [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0]],
            [[1.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
            [[1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
            [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 0.0, 1.0]],
            [[0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]],
            [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0]],
            [[0.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        ];
        for f in faces {
            out.extend_from_slice(&0f32.to_le_bytes());
            out.extend_from_slice(&0f32.to_le_bytes());
            out.extend_from_slice(&0f32.to_le_bytes());
            for v in f {
                for c in v {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
            out.extend_from_slice(&0u16.to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_binary_cube() {
        let mesh = parse(&cube_binary()).unwrap();
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertex_count(), 36);
        assert_eq!(mesh.bounds().size().max_element(), 1.0);
    }

    #[test]
    fn parses_ascii_cube() {
        let text = "solid cube\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid cube\n";
        let mesh = parse(text.as_bytes()).unwrap();
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn rejects_truncated_binary() {
        let mut bytes = cube_binary();
        bytes.truncate(bytes.len() - 10);
        assert!(parse(&bytes).is_err());
    }
}
