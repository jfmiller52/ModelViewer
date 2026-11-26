#!/usr/bin/env python3
"""Author the synthetic sample models shipped with ModelView.

Everything here is generated geometry; there are no third-party model files.
Run from the repo root:  python3 scripts/make_samples.py
"""

import math
import os
import struct
import zipfile
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLES = os.path.join(ROOT, "samples")
os.makedirs(SAMPLES, exist_ok=True)


def write_binary_stl(path, triangles):
    with open(path, "wb") as f:
        f.write(b"ModelView synthetic sample".ljust(80, b" "))
        f.write(struct.pack("<I", len(triangles)))
        for a, b, c in triangles:
            u = [b[i] - a[i] for i in range(3)]
            v = [c[i] - a[i] for i in range(3)]
            n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
            length = math.sqrt(sum(x * x for x in n)) or 1.0
            f.write(struct.pack("<3f", *(x / length for x in n)))
            for p in (a, b, c):
                f.write(struct.pack("<3f", *p))
            f.write(b"\x00\x00")


def validate(name, triangles):
    edges = defaultdict(int)
    directed = defaultdict(int)
    volume = 0.0
    for a, b, c in triangles:
        for u, v in ((a, b), (b, c), (c, a)):
            key = (min(u, v), max(u, v))
            edges[key] += 1
            directed[(u, v)] += 1
        volume += (
            a[0] * (b[1] * c[2] - b[2] * c[1])
            - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
        ) / 6.0
    open_edges = sum(1 for v in edges.values() if v == 1)
    nonmanifold = sum(1 for v in edges.values() if v > 2)
    flipped = sum(1 for k, v in directed.items() if v > 1)
    print(f"{name}: {len(triangles)} tris, open {open_edges}, non-manifold {nonmanifold}, flipped {flipped}, volume {volume:.1f} mm3")
    return open_edges, nonmanifold, volume


CTRL = [
    (0.00, 26.0),
    (0.10, 33.0),
    (0.30, 40.5),
    (0.50, 40.0),
    (0.68, 34.0),
    (0.82, 28.0),
    (0.92, 29.5),
    (1.00, 34.0),
]


def profile(t):
    for i in range(len(CTRL) - 1):
        t0, r0 = CTRL[i]
        t1, r1 = CTRL[i + 1]
        if t0 <= t <= t1:
            u = (t - t0) / (t1 - t0)
            s = u * u * (3.0 - 2.0 * u)
            return r0 + (r1 - r0) * s
    return CTRL[-1][1]


def radius(t, theta):
    base = profile(t)
    rib = 1.0 + 0.032 * math.sin(math.pi * min(max(t, 0.0), 1.0)) ** 0.7 * math.cos(14.0 * theta)
    return base * rib


def vase():
    segments = 96
    rings = 110
    height = 120.0
    wall = 3.2
    cavity_floor = 4.0
    positions = []
    tris = []

    def add(p):
        positions.append(p)
        return len(positions) - 1

    outer = [[0] * segments for _ in range(rings + 1)]
    for i in range(rings + 1):
        t = i / rings
        z = height * t
        for j in range(segments):
            th = 2.0 * math.pi * j / segments
            r = radius(t, th)
            outer[i][j] = add((r * math.cos(th), r * math.sin(th), z))

    inner_rings = 104
    inner = [[0] * segments for _ in range(inner_rings + 1)]
    for i in range(inner_rings + 1):
        t = i / inner_rings
        z = cavity_floor + (height - cavity_floor) * t
        tr = (z / height)
        for j in range(segments):
            th = 2.0 * math.pi * j / segments
            r = max(radius(tr, th) - wall, 3.0)
            inner[i][j] = add((r * math.cos(th), r * math.sin(th), z))

    hole_rings = range(52, 58)
    hole_segs = set(range(62, 68))

    # Outer wall, outward.
    for i in range(rings):
        for j in range(segments):
            if i in hole_rings and j in hole_segs:
                continue
            jn = (j + 1) % segments
            a, b, c, d = outer[i][j], outer[i][jn], outer[i + 1][jn], outer[i + 1][j]
            tris.append((positions[a], positions[b], positions[c]))
            tris.append((positions[a], positions[c], positions[d]))

    # Inner wall, outward from the material (towards the cavity axis).
    for i in range(inner_rings):
        for j in range(segments):
            jn = (j + 1) % segments
            a, b, c, d = inner[i][j], inner[i][jn], inner[i + 1][jn], inner[i + 1][j]
            tris.append((positions[a], positions[c], positions[b]))
            tris.append((positions[a], positions[d], positions[c]))

    # Outer bottom, facing down.
    center_bottom = add((0.0, 0.0, 0.0))
    for j in range(segments):
        jn = (j + 1) % segments
        a, b = outer[0][j], outer[0][jn]
        tris.append((positions[center_bottom], positions[b], positions[a]))

    # Cavity floor, facing up into the bowl.
    center_cavity = add((0.0, 0.0, cavity_floor))
    for j in range(segments):
        jn = (j + 1) % segments
        a, b = inner[0][j], inner[0][jn]
        tris.append((positions[center_cavity], positions[a], positions[b]))

    # Rim annulus at the top, facing up.
    for j in range(segments):
        jn = (j + 1) % segments
        a, b = outer[rings][j], outer[rings][jn]
        c, d = inner[inner_rings][jn], inner[inner_rings][j]
        tris.append((positions[a], positions[b], positions[c]))
        tris.append((positions[a], positions[c], positions[d]))

    # One flipped triangle on the back left, so the report has a winding defect.
    flip_index = None
    for index, (a, b, c) in enumerate(tris):
        if a[2] > 40 and b[2] > 40 and c[2] > 40 and a[1] > 0 and a[0] < -10:
            flip_index = index
            break
    if flip_index is not None:
        a, b, c = tris[flip_index]
        tris[flip_index] = (a, c, b)
    return tris


def torus():
    major, minor = 24.0, 7.0
    u_seg, v_seg = 72, 40
    tris = []
    pts = {}
    for i in range(u_seg):
        u = 2.0 * math.pi * i / u_seg
        for j in range(v_seg):
            v = 2.0 * math.pi * j / v_seg
            r = major + minor * math.cos(v)
            pts[(i, j)] = (r * math.cos(u), r * math.sin(u), minor * math.sin(v) + minor)
    for i in range(u_seg):
        for j in range(v_seg):
            i2, j2 = (i + 1) % u_seg, (j + 1) % v_seg
            a, b, c, d = pts[(i, j)], pts[(i2, j)], pts[(i2, j2)], pts[(i, j2)]
            tris.append((a, b, c))
            tris.append((a, c, d))
    return tris


def cylinder(radius_mm, height, segments=72):
    tris = []
    pts = {}
    for i in range(segments):
        th = 2.0 * math.pi * i / segments
        x, y = radius_mm * math.cos(th), radius_mm * math.sin(th)
        pts[(i, 0)] = (x, y, 0.0)
        pts[(i, 1)] = (x, y, height)
    for i in range(segments):
        i2 = (i + 1) % segments
        a, b = pts[(i, 0)], pts[(i2, 0)]
        c, d = pts[(i2, 1)], pts[(i, 1)]
        tris.append((a, b, c))
        tris.append((a, c, d))
    bottom = (0.0, 0.0, 0.0)
    top = (0.0, 0.0, height)
    for i in range(segments):
        i2 = (i + 1) % segments
        tris.append((bottom, pts[(i2, 0)], pts[(i, 0)]))
        tris.append((top, pts[(i, 1)], pts[(i2, 1)]))
    return tris


def mesh_xml(triangles):
    index = {}
    vertices = []
    out = []
    for tri in triangles:
        ids = []
        for p in tri:
            key = (round(p[0], 5), round(p[1], 5), round(p[2], 5))
            if key not in index:
                index[key] = len(vertices)
                vertices.append(key)
            ids.append(index[key])
        out.append(ids)
    verts = "".join(f'<vertex x="{p[0]}" y="{p[1]}" z="{p[2]}"/>' for p in vertices)
    faces = "".join(f'<triangle v1="{a}" v2="{b}" v3="{c}"/>' for a, b, c in out)
    return f"<mesh><vertices>{verts}</vertices><triangles>{faces}</triangles></mesh>", len(vertices), len(out)


def write_3mf(path):
    torus_xml, torus_v, torus_f = mesh_xml(torus())
    cyl_xml, cyl_v, cyl_f = mesh_xml(cylinder(12.0, 34.0))
    model = f"""<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <metadata name="Application">ModelView sample author</metadata>
  <metadata name="Title">Two-object print project</metadata>
  <resources>
    <object id="1" type="model" name="Torus ring">{torus_xml}</object>
    <object id="2" type="model" name="Spool hub">{cyl_xml}</object>
  </resources>
  <build>
    <item objectid="1" transform="1 0 0 0 1 0 0 0 1 -28 0 0"/>
    <item objectid="2" transform="1 0 0 0 1 0 0 0 1 28 0 0"/>
  </build>
</model>"""
    rels = """<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"""
    content_types = """<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
  <Default Extension="config" ContentType="application/octet-stream"/>
</Types>"""
    model_settings = """<?xml version="1.0" encoding="UTF-8"?>
<config>
  <object id="1"><metadata key="name" value="Torus ring"/><metadata key="plater_id" value="1"/></object>
  <object id="2"><metadata key="name" value="Spool hub"/><metadata key="plater_id" value="2"/></object>
  <plate>
    <metadata key="plater_id" value="1"/>
    <metadata key="plater_name" value="Plate 01"/>
    <model_instance><metadata key="object_id" value="1"/><metadata key="instance_id" value="0"/></model_instance>
  </plate>
  <plate>
    <metadata key="plater_id" value="2"/>
    <metadata key="plater_name" value="Plate 02"/>
    <model_instance><metadata key="object_id" value="2"/><metadata key="instance_id" value="0"/></model_instance>
  </plate>
</config>"""
    project_settings = """{
  "printer_model": "Bambu Lab X1 Carbon",
  "printer_settings_id": "0.20mm Standard @BBL X1C",
  "filament_type": ["PLA"],
  "layer_height": "0.2"
}"""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", content_types)
        z.writestr("_rels/.rels", rels)
        z.writestr("3D/3dmodel.model", model)
        z.writestr("Metadata/model_settings.config", model_settings)
        z.writestr("Metadata/project_settings.config", project_settings)
    print(f"3MF: torus {torus_f} tris / {torus_v} verts, hub {cyl_f} tris / {cyl_v} verts")


def main():
    broken = vase()
    validate("vase_broken", broken)
    write_binary_stl(os.path.join(SAMPLES, "vase_broken.stl"), broken)

    write_3mf(os.path.join(SAMPLES, "project.3mf"))


if __name__ == "__main__":
    main()
