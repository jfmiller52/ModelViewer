struct Globals {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera: vec4<f32>,
    params: vec4<f32>,
    focus: vec4<f32>,
    focus_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct ObjectData {
    base: vec4<f32>,
    flags: vec4<f32>,
};

@group(1) @binding(0) var<uniform> object: ObjectData;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let model = mat3x3<f32>(
        globals.model[0].xyz,
        globals.model[1].xyz,
        globals.model[2].xyz
    );
    out.world = (globals.model * vec4<f32>(in.position, 1.0)).xyz;
    out.normal = model * in.normal;
    out.clip = globals.view_proj * vec4<f32>(out.world, 1.0);
    return out;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = c < vec3<f32>(0.0031308);
    let lower = c * 12.92;
    let higher = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(higher, lower, cutoff);
}

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    let view = normalize(globals.camera.xyz - in.world);
    let key = normalize(vec3<f32>(0.38, 0.55, 0.74));
    let fill = normalize(vec3<f32>(-0.62, -0.18, 0.36));
    let ndl = max(dot(n, key), 0.0);
    let ndf = max(dot(n, fill), 0.0);
    let hemi = 0.5 + 0.5 * n.z;
    let base = object.base.rgb;
    var col = base * (0.18 + 0.64 * ndl + 0.14 * ndf + 0.16 * hemi);
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.2) * 0.14;
    col += base * rim;
    if (object.flags.x > 0.5) {
        col = mix(col, vec3<f32>(0.18, 0.42, 1.0), 0.22);
    }

    let focus_amount = globals.focus_color.a;
    if (focus_amount > 0.001) {
        let ghost = vec3<f32>(0.008, 0.012, 0.020) + base * 0.012;
        let rim = pow(1.0 - max(dot(n, view), 0.0), 2.4);
        let ghost_col = ghost + vec3<f32>(0.018, 0.028, 0.045) * rim;
        col = mix(col, ghost_col, focus_amount);
    }
    return vec4<f32>(linear_to_srgb(col), 1.0);
}
