//! 3D board for the Wayland client.
//!
//! The meshes, Wood textures, and `board.wgsl` scene are the same ones
//! `chess-linux` draws with wgpu. This module only packs that scene. Wawona
//! executes it through `wawona_vk_*` and the guest copies the RGBA frame into
//! the existing `wl_shm` buffer. No Vulkan symbol on the host means the
//! software board stays.

use crate::game::Game;
use glam::{Mat4, Vec3, Vec4};
use shakmaty::{Color, Move, Position, Role, Square};
use std::sync::OnceLock;

const PIECE_MESHES: [&[u8]; 6] = [
    include_bytes!("../../assets/pawn.mesh"),
    include_bytes!("../../assets/knight.mesh"),
    include_bytes!("../../assets/bishop.mesh"),
    include_bytes!("../../assets/rook.mesh"),
    include_bytes!("../../assets/queen.mesh"),
    include_bytes!("../../assets/king.mesh"),
];

#[cfg(not(test))]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn wawona_vk_probe() -> i32;
    fn wawona_vk_upload(desc: *const u8) -> i32;
    fn wawona_vk_frame(desc: *const u8) -> i32;
}

pub struct Cam {
    pub yaw: f32,
    pub elevation: f32,
    pub distance: f32,
}

impl Default for Cam {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            elevation: 55.0,
            distance: 13.7,
        }
    }
}

pub fn viewport(width: i32, height: i32) -> Option<(i32, i32, i32, i32)> {
    let x = 8;
    let y = 80;
    let w = (width - 16).max(0) & !1;
    let h = (height - 80 - 88).max(0) & !1;
    if w < 64 || h < 64 {
        None
    } else {
        Some((x, y, w, h))
    }
}

pub fn available() -> bool {
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        unsafe { wawona_vk_probe() == 1 }
    }
}

pub fn shown(width: i32, height: i32) -> bool {
    available() && ensure_uploaded() && viewport(width, height).is_some()
}

struct Mesh {
    start: u32,
    count: u32,
}

struct Assets {
    verts: Vec<u8>,
    meshes: Vec<Mesh>,
    blobs: Vec<u8>,
    table: Vec<u32>,
}

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(build_assets)
}

fn build_assets() -> Assets {
    let mut verts = Vec::new();
    let mut meshes = Vec::new();
    for bytes in PIECE_MESHES {
        let start = (verts.len() / 32) as u32;
        verts.extend_from_slice(bytes);
        meshes.push(Mesh {
            start,
            count: (verts.len() / 32) as u32 - start,
        });
    }
    let start = (verts.len() / 32) as u32;
    plane(&mut verts);
    meshes.push(Mesh {
        start,
        count: (verts.len() / 32) as u32 - start,
    });
    let start = (verts.len() / 32) as u32;
    cube(&mut verts);
    meshes.push(Mesh {
        start,
        count: (verts.len() / 32) as u32 - start,
    });
    let start = (verts.len() / 32) as u32;
    disc(&mut verts);
    meshes.push(Mesh {
        start,
        count: (verts.len() / 32) as u32 - start,
    });
    let (blobs, table) = textures();
    Assets {
        verts,
        meshes,
        blobs,
        table,
    }
}

fn textures() -> (Vec<u8>, Vec<u32>) {
    let files: [&[u8]; 20] = [
        include_bytes!("../../Styles/Wood/WhiteBoard.png"),
        include_bytes!("../../Styles/Wood/BlackBoard.png"),
        include_bytes!("../../Styles/Wood/Border.png"),
        include_bytes!("../../Styles/Marble/WhiteBoard.png"),
        include_bytes!("../../Styles/Marble/BlackBoard.png"),
        include_bytes!("../../Styles/Marble/Border.png"),
        include_bytes!("../../Styles/Metal/WhiteBoard.png"),
        include_bytes!("../../Styles/Metal/BlackBoard.png"),
        include_bytes!("../../Styles/Metal/Border.png"),
        include_bytes!("../../Styles/Grass/WhiteBoard.png"),
        include_bytes!("../../Styles/Grass/BlackBoard.png"),
        include_bytes!("../../Styles/Grass/Border.png"),
        include_bytes!("../../Resources/MTL/PiecesWhite/WhitePieceWood.jpg"),
        include_bytes!("../../Resources/MTL/PiecesBlack/BlackPieceWood.jpg"),
        include_bytes!("../../Resources/MTL/PiecesWhite/WhitePieceMarble.jpg"),
        include_bytes!("../../Resources/MTL/PiecesBlack/BlackPieceMarble.jpg"),
        include_bytes!("../../Resources/MTL/PiecesWhite/WhitePieceMetal.jpg"),
        include_bytes!("../../Resources/MTL/PiecesBlack/BlackPieceMetal.jpg"),
        include_bytes!("../../Styles/Fur/WhitePiece.png"),
        include_bytes!("../../Styles/Fur/BlackPiece.png"),
    ];
    let mut blobs = Vec::new();
    let mut table = Vec::new();
    for file in files {
        table.push(blobs.len() as u32);
        table.push(file.len() as u32);
        blobs.extend_from_slice(file);
    }
    (blobs, table)
}

fn ensure_uploaded() -> bool {
    static UPLOADED: OnceLock<bool> = OnceLock::new();
    *UPLOADED.get_or_init(|| {
        let assets = assets();
        let spirv = include_bytes!(concat!(env!("OUT_DIR"), "/board.spv"));
        let mut desc = Vec::new();
        push_u32(&mut desc, spirv.as_ptr() as u32);
        push_u32(&mut desc, spirv.len() as u32);
        push_u32(&mut desc, assets.verts.as_ptr() as u32);
        push_u32(&mut desc, assets.verts.len() as u32);
        push_u32(&mut desc, assets.blobs.as_ptr() as u32);
        push_u32(&mut desc, assets.blobs.len() as u32);
        push_u32(&mut desc, assets.table.as_ptr() as u32);
        push_u32(&mut desc, (assets.table.len() / 2) as u32);
        #[cfg(test)]
        {
            let _ = desc;
            false
        }
        #[cfg(not(test))]
        {
            let rc = unsafe { wawona_vk_upload(desc.as_ptr()) };
            if rc != 0 {
                eprintln!("chess-wawona: vulkan upload errno={rc}");
            }
            rc == 0
        }
    })
}

pub fn frame(
    g: &Game,
    cam: &Cam,
    selected: Option<Square>,
    width: i32,
    height: i32,
) -> Option<Vec<u8>> {
    if width < 64 || height < 64 || !available() || !ensure_uploaded() {
        return None;
    }
    let (instances, draws) = scene(g, selected);
    let uniform = camera_bytes(cam, width as f32, height as f32);
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let mut desc = Vec::new();
    push_u32(&mut desc, instances.as_ptr() as u32);
    push_u32(&mut desc, instances.len() as u32);
    push_u32(&mut desc, draws.as_ptr() as u32);
    push_u32(&mut desc, draws.len() as u32);
    push_u32(&mut desc, uniform.as_ptr() as u32);
    push_u32(&mut desc, uniform.len() as u32);
    push_u32(&mut desc, width as u32);
    push_u32(&mut desc, height as u32);
    push_u32(&mut desc, pixels.as_mut_ptr() as u32);
    push_u32(&mut desc, pixels.len() as u32);
    submit_frame(&desc, pixels)
}

fn submit_frame(desc: &[u8], pixels: Vec<u8>) -> Option<Vec<u8>> {
    #[cfg(test)]
    {
        let _ = (desc, pixels);
        None
    }
    #[cfg(not(test))]
    {
        let rc = unsafe { wawona_vk_frame(desc.as_ptr()) };
        if rc != 0 {
            eprintln!("chess-wawona: vulkan frame errno={rc}");
            return None;
        }
        Some(pixels)
    }
}

fn scene(g: &Game, selected: Option<Square>) -> (Vec<u8>, Vec<u8>) {
    let meshes = &assets().meshes;
    let mut batches: Vec<Vec<u8>> = vec![Vec::new(); meshes.len()];
    let mut add = |mesh: usize,
                   pos: Vec3,
                   scale: Vec3,
                   yaw: f32,
                   tint: [f32; 4],
                   layer: f32,
                   rough: f32,
                   unlit: bool| {
        let model =
            Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(scale);
        push_instance(&mut batches[mesh], model, tint, layer, rough, unlit, mesh);
    };
    let board = 0;
    add(
        7,
        Vec3::new(0.0, -0.22, 0.0),
        Vec3::new(9.1, 0.42, 9.1),
        0.0,
        [0.55, 0.55, 0.55, 1.0],
        (board + 2) as f32,
        0.4,
        false,
    );
    for sq in Square::ALL {
        let p = square_world(sq);
        let dark = (i32::from(sq.rank()) + i32::from(sq.file())) % 2 == 0;
        add(
            6,
            p,
            Vec3::ONE,
            0.0,
            [1.0; 4],
            (board + usize::from(dark)) as f32,
            0.4,
            false,
        );
    }
    let legal = g.board.moves();
    if let Some(sq) = selected {
        add(
            6,
            square_world(sq) + Vec3::Y * 0.009,
            Vec3::splat(0.98),
            0.0,
            [0.9, 0.7, 0.25, 0.32],
            -1.0,
            1.0,
            true,
        );
        for m in legal.iter().filter(|m| m.from() == Some(sq)) {
            add(
                8,
                square_world(destination(*m)) + Vec3::Y * 0.014,
                Vec3::splat(if m.is_capture() { 0.43 } else { 0.11 }),
                0.0,
                [0.8, 0.85, 0.65, 0.45],
                -1.0,
                1.0,
                true,
            );
        }
    }
    if let Some(m) = last_move(g)
        && let Some(from) = m.from()
    {
        let a = square_world(from);
        let b = square_world(destination(m));
        let delta = b - a;
        let angle = delta.x.atan2(delta.z);
        let len = delta.length().max(0.01);
        let color = [0.85, 0.75, 0.35, 0.58];
        add(
            6,
            (a + b) * 0.5 + Vec3::Y * 0.025,
            Vec3::new(0.1, 1.0, len),
            angle,
            color,
            -1.0,
            1.0,
            true,
        );
        add(
            6,
            b + Vec3::Y * 0.028,
            Vec3::new(0.26, 1.0, 0.26),
            std::f32::consts::FRAC_PI_4,
            color,
            -1.0,
            1.0,
            true,
        );
    }
    for (sq, piece) in g.board.pos.board().iter() {
        let pos = square_world(sq);
        add(
            8,
            pos + Vec3::new(0.07, 0.004, -0.06),
            Vec3::new(0.44, 1.0, 0.44),
            0.0,
            [0.0, 0.0, 0.0, 0.32],
            -1.0,
            1.0,
            true,
        );
        let mesh = match piece.role {
            Role::Pawn => 0,
            Role::Knight => 1,
            Role::Bishop => 2,
            Role::Rook => 3,
            Role::Queen => 4,
            Role::King => 5,
        };
        let layer = 12 + usize::from(piece.color == Color::Black);
        let yaw = if (piece.role == Role::Knight && piece.color == Color::Black)
            || (piece.role == Role::Bishop && piece.color == Color::White)
        {
            std::f32::consts::PI
        } else {
            0.0
        };
        add(
            mesh,
            pos,
            Vec3::ONE,
            yaw,
            [1.0; 4],
            layer as f32,
            0.45,
            false,
        );
    }

    let mut instances = Vec::new();
    let mut ranges = Vec::new();
    for batch in &batches {
        let start = (instances.len() / 112) as u32;
        instances.extend_from_slice(batch);
        ranges.push(start..(instances.len() / 112) as u32);
    }
    let mut draws = Vec::new();
    for mesh in 0..6 {
        let reflected_start = (instances.len() / 112) as u32;
        let model_bytes = &batches[mesh];
        for chunk in model_bytes.chunks_exact(112) {
            let model = mat_from_bytes(chunk);
            let flipped = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * model;
            let mut out = Vec::with_capacity(112);
            push_mat(&mut out, flipped);
            out.extend_from_slice(&chunk[64..]);
            instances.extend_from_slice(&out);
        }
        let end = (instances.len() / 112) as u32;
        push_draw(
            &mut draws,
            0,
            meshes[mesh].start,
            meshes[mesh].count,
            reflected_start,
            end - reflected_start,
        );
    }
    for order in [7usize, 6, 8, 0, 1, 2, 3, 4, 5] {
        let range = &ranges[order];
        push_draw(
            &mut draws,
            1,
            meshes[order].start,
            meshes[order].count,
            range.start,
            range.end - range.start,
        );
    }
    (instances, draws)
}

fn last_move(g: &Game) -> Option<Move> {
    let i = g.data.cursor.checked_sub(1)?;
    let text = g.data.moves.get(i)?;
    g.states.get(i)?.parse_move(text).ok()
}

fn push_instance(
    out: &mut Vec<u8>,
    model: Mat4,
    tint: [f32; 4],
    layer: f32,
    rough: f32,
    unlit: bool,
    mesh: usize,
) {
    push_mat(out, model);
    for x in tint {
        push_f32(out, x);
    }
    push_f32(out, layer);
    push_f32(out, rough);
    push_f32(out, if unlit { 1.0 } else { 0.0 });
    push_f32(out, if mesh == 6 && !unlit { 1.0 } else { 0.0 });
    for x in if unlit {
        [1.0, 0.0, 1.0, 1.0]
    } else {
        [0.68, 0.55, 35.0, 1.0]
    } {
        push_f32(out, x);
    }
}

fn camera_bytes(cam: &Cam, width: f32, height: f32) -> Vec<u8> {
    let (vp, eye) = matrices(cam, width / height);
    let mut out = Vec::with_capacity(144);
    push_mat(&mut out, vp);
    for x in eye.extend(1.0).to_array() {
        push_f32(&mut out, x);
    }
    for x in [239.0 / 255.0, 235.0 / 255.0, 225.0 / 255.0, 1.0] {
        push_f32(&mut out, x);
    }
    for x in [-6.0, 12.0, 8.0, 1.0] {
        push_f32(&mut out, x);
    }
    for x in [0.38, 0.25, 0.9, 0.0] {
        push_f32(&mut out, x);
    }
    for x in [width, height, 0.0, 0.0] {
        push_f32(&mut out, x);
    }
    out
}

fn matrices(cam: &Cam, aspect: f32) -> (Mat4, Vec3) {
    let yaw = cam.yaw.to_radians();
    let tilt = cam.elevation.to_radians();
    let eye = Vec3::new(yaw.sin() * tilt.cos(), tilt.sin(), yaw.cos() * tilt.cos())
        * cam.distance
        * (1.05 / aspect).max(1.0);
    let vp = Mat4::perspective_rh(40.0f32.to_radians(), aspect, 0.1, 100.0)
        * Mat4::look_at_rh(eye, Vec3::new(0.0, 0.3, 0.0), Vec3::Y);
    (vp, eye)
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn pick(cam: &Cam, point: [f32; 2], rect: Rect, g: &Game) -> Option<Square> {
    let [x, y] = point;
    let Rect {
        x: left,
        y: top,
        w: width,
        h: height,
    } = rect;
    let (vp, _) = matrices(cam, width / height);
    let inv = vp.inverse();
    let nx = 2.0 * (x - left) / width - 1.0;
    let ny = 1.0 - 2.0 * (y - top) / height;
    let near = inv * Vec4::new(nx, ny, 0.0, 1.0);
    let far = inv * Vec4::new(nx, ny, 1.0, 1.0);
    let a = near.truncate() / near.w;
    let b = far.truncate() / far.w;
    let ray = (b - a).normalize();
    let mut nearest = f32::MAX;
    let mut picked = None;
    for (sq, piece) in g.board.pos.board().iter() {
        let height = match piece.role {
            Role::Pawn => 1.0,
            Role::Rook => 1.1,
            Role::Knight | Role::Bishop => 1.5,
            _ => 1.8,
        };
        let center = square_world(sq) + Vec3::Y * height * 0.45;
        let delta = a - center;
        let bb = delta.dot(ray);
        let cc = delta.length_squared() - 0.34 * 0.34;
        let disc = bb * bb - cc;
        if disc >= 0.0 {
            let t = -bb - disc.sqrt();
            if t > 0.0 && t < nearest {
                nearest = t;
                picked = Some(sq);
            }
        }
    }
    if picked.is_some() {
        return picked;
    }
    if ray.y.abs() < 0.001 {
        return None;
    }
    let t = -a.y / ray.y;
    if t < 0.0 {
        return None;
    }
    let hit = a + ray * t;
    let file = (hit.x + 4.0).floor() as i32;
    let rank = (4.0 - hit.z).floor() as i32;
    if (0..8).contains(&file) && (0..8).contains(&rank) {
        Some(Square::new((rank * 8 + file) as u32))
    } else {
        None
    }
}

fn square_world(sq: Square) -> Vec3 {
    Vec3::new(
        i32::from(sq.file()) as f32 - 3.5,
        0.0,
        3.5 - i32::from(sq.rank()) as f32,
    )
}

fn destination(m: Move) -> Square {
    if let Move::Castle { king, rook } = m {
        Square::new(u32::from(king.rank()) * 8 + if rook.file() > king.file() { 6 } else { 2 })
    } else {
        m.to()
    }
}

fn push_vert(out: &mut Vec<u8>, p: [f32; 3], n: [f32; 3], uv: [f32; 2]) {
    for x in p.into_iter().chain(n).chain(uv) {
        push_f32(out, x);
    }
}

fn plane(v: &mut Vec<u8>) {
    for (p, uv) in [
        ([-0.5, 0.0, -0.5], [0.0, 0.0]),
        ([0.5, 0.0, -0.5], [1.0, 0.0]),
        ([0.5, 0.0, 0.5], [1.0, 1.0]),
        ([-0.5, 0.0, -0.5], [0.0, 0.0]),
        ([0.5, 0.0, 0.5], [1.0, 1.0]),
        ([-0.5, 0.0, 0.5], [0.0, 1.0]),
    ] {
        push_vert(v, p, [0.0, 1.0, 0.0], uv);
    }
}

fn cube(v: &mut Vec<u8>) {
    let corners = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    for (ids, n) in [
        ([0, 1, 2, 3], [0.0, 0.0, -1.0]),
        ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
        ([0, 4, 7, 3], [-1.0, 0.0, 0.0]),
        ([1, 5, 6, 2], [1.0, 0.0, 0.0]),
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ] {
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for i in [0, 1, 2, 0, 2, 3] {
            push_vert(v, corners[ids[i]], n, uvs[i]);
        }
    }
}

fn disc(v: &mut Vec<u8>) {
    for i in 0..48 {
        let a = i as f32 * std::f32::consts::TAU / 48.0;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 48.0;
        for p in [
            [0.0, 0.0, 0.0],
            [a.cos(), 0.0, a.sin()],
            [b.cos(), 0.0, b.sin()],
        ] {
            push_vert(v, p, [0.0, 1.0, 0.0], [0.0, 0.0]);
        }
    }
}

fn push_mat(out: &mut Vec<u8>, m: Mat4) {
    for col in m.to_cols_array_2d() {
        for x in col {
            push_f32(out, x);
        }
    }
}

fn mat_from_bytes(bytes: &[u8]) -> Mat4 {
    let mut cols = [[0.0f32; 4]; 4];
    for (i, col) in cols.iter_mut().enumerate() {
        for (j, x) in col.iter_mut().enumerate() {
            let o = (i * 4 + j) * 4;
            *x = f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        }
    }
    Mat4::from_cols_array_2d(&cols)
}

fn push_draw(out: &mut Vec<u8>, pass: u32, v0: u32, vc: u32, i0: u32, ic: u32) {
    for x in [pass, v0, vc, i0, ic] {
        push_u32(out, x);
    }
}

fn push_f32(out: &mut Vec<u8>, x: f32) {
    out.extend_from_slice(&x.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, x: u32) {
    out.extend_from_slice(&x.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Rules;

    #[test]
    fn default_camera_picks_both_sides() {
        let game = Game::new(Rules::Standard);
        let cam = Cam::default();
        let (left, top, width, height) = (0.0, 60.0, 1000.0, 730.0);
        for yaw in [0.0, 180.0] {
            let cam = Cam { yaw, ..cam };
            for sq in [
                Square::E2,
                Square::D2,
                Square::E7,
                Square::D7,
                Square::E4,
                Square::D5,
            ] {
                let point =
                    square_world(sq) + Vec3::Y * if game.at(sq).is_some() { 0.45 } else { 0.0 };
                let (vp, _) = matrices(&cam, width / height);
                let p = vp * point.extend(1.0);
                let p = p.truncate() / p.w;
                let px = left + (p.x + 1.0) * 0.5 * width;
                let py = top + (1.0 - p.y) * 0.5 * height;
                assert_eq!(
                    pick(
                        &cam,
                        [px, py],
                        Rect {
                            x: left,
                            y: top,
                            w: width,
                            h: height,
                        },
                        &game,
                    ),
                    Some(sq),
                    "{yaw} {sq}"
                );
            }
        }
    }
}
