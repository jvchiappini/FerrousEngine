/// Frustum culling — CPU-side visibility test for axis-aligned bounding boxes.
///
/// ## Algorithm
///
/// Given the combined `view_proj` matrix `M`, the six frustum planes in
/// clip space can be extracted analytically (Gribb-Hartmann method).  Each
/// plane is stored as a `Vec4(nx, ny, nz, d)` in world space.
///
/// For an AABB to be **completely outside** one plane it is sufficient to
/// show that its *positive vertex* (the corner farthest in the plane's normal
/// direction) has a negative signed distance to the plane.  If no such plane
/// exists, the AABB is considered **visible**.
///
/// This is O(6) per object and branchless-friendly, making it cheap enough
/// to run on the CPU every frame before building `DrawCommand`s.
use glam::{Mat4, Vec3, Vec4};

// ── AABB ─────────────────────────────────────────────────────────────────────

/// World-space axis-aligned bounding box.
#[derive(Copy, Clone, Debug)]
pub struct Aabb {
    pub center: Vec3,
    pub half_extents: Vec3,
}

impl Aabb {
    /// Creates an AABB from `min`/`max` corners.
    #[inline]
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { 
            center: (min + max) * 0.5, 
            half_extents: (max - min) * 0.5 
        }
    }

    /// Creates a unit-cube AABB centred at `origin` (fits the built-in cube mesh).
    #[inline]
    pub fn unit_cube() -> Self {
        Self { 
            center: Vec3::ZERO, 
            half_extents: Vec3::splat(1.0) 
        }
    }

    /// Returns a new AABB transformed by `transform` (world-space position/scale).
    ///
    /// Transforms all 8 corners and takes the new min/max — correct even for
    /// non-uniform scale and arbitrary rotations, though we only use axis-aligned
    /// transforms for now.
    pub fn transform(&self, transform: &Mat4) -> Self {
        // Fast AABB transform: transform centre + half-extents (avoids 8-corner loop).        
        // Source: Graphics Gems (Arvo 1990).
        let new_centre = transform.transform_point3(self.center);

        // Absolute-value of upper-left 3×3 rotates the half-extents.
        let m = transform.to_cols_array_2d();
        let half = self.half_extents;
        let new_half = Vec3::new(
            half.x * m[0][0].abs() + half.y * m[1][0].abs() + half.z * m[2][0].abs(),
            half.x * m[0][1].abs() + half.y * m[1][1].abs() + half.z * m[2][1].abs(),
            half.x * m[0][2].abs() + half.y * m[1][2].abs() + half.z * m[2][2].abs(),
        );

        Self {
            center: new_centre,
            half_extents: new_half,
        }
    }
}// ── Frustum ───────────────────────────────────────────────────────────────────

/// Six clip planes extracted from a `view_proj` matrix.
///
/// Each plane is stored as `Vec4(nx, ny, nz, d)` where the plane equation is
/// `dot(normal, point) + d >= 0` for visible points.
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// Extracts the six frustum planes from `view_proj` (column-major).
    ///
    /// Uses the Gribb-Hartmann row-combination method.  Works for both
    /// left-handed (wgpu/Vulkan) and right-handed conventions because the
    /// signs cancel correctly.
    pub fn from_view_proj(vp: &Mat4) -> Self {
        let m = vp.to_cols_array_2d(); // m[col][row]

        // Row vectors of the matrix (convenient for plane extraction).
        let row = |r: usize| Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]);

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        // Planes: left, right, bottom, top, near, far.
        // Negate far for wgpu's [0,1] depth range (reversed-Z style).
        let mut planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r2,      // near  (wgpu: depth in [0,1])
            r3 - r2, // far
        ];

        // Normalise so that the signed distance formula is meaningful.
        for p in &mut planes {
            let len = Vec3::new(p.x, p.y, p.z).length();
            if len > 1e-6 {
                *p /= len;
            }
        }

        Self { planes }
    }

    /// Returns `true` if the AABB **might** be visible (conservative — no false negatives).
    ///
    /// Uses the positive-vertex / negative-vertex test: if the positive vertex
    /// (closest to the plane's outward normal) is behind the plane, the whole
    /// AABB is outside the frustum.
    #[inline]
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        for plane in &self.planes {
            // Branchless AABB / plane test:
            // dot(center, normal) + dot(half_extents, abs(normal)) + d
            let center_dist = plane.x * aabb.center.x + plane.y * aabb.center.y + plane.z * aabb.center.z + plane.w;
            let extent_dist = plane.x.abs() * aabb.half_extents.x + plane.y.abs() * aabb.half_extents.y + plane.z.abs() * aabb.half_extents.z;

            // If the maximum inscribed sphere distance goes below 0, it's outside.
            // Wait, more precisely, if the closest point is outside, the whole box is outside.
            // closest_dist = center_dist + extent_dist. If that is < 0, it's outside.
            if center_dist + extent_dist < 0.0 {
                return false; // completely outside this plane
            }
        }
        true
    }

    /// Returns the six frustum planes as `[Vec4; 6]`.
    ///
    /// Each plane is `(nx, ny, nz, d)` in the form `dot(n, p) + d >= 0`.
    #[inline]
    pub fn planes(&self) -> &[Vec4; 6] {
        &self.planes
    }
}
