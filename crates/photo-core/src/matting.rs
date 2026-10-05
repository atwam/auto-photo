//! Matte refinement at output resolution: a colour guided filter snaps the
//! (upsampled, soft) network matte to real edges, and a local background
//! estimate lets compositing remove the old background's colour from hair.

/// Mean over a (2r+1)² window, clamped at the borders. Separable running sums.
pub fn box_mean(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let mut acc = 0.0f64;
        let mut n = 0usize;
        // Window [x-r, x+r]; prime with [0, r].
        for v in row.iter().take(r.min(w - 1) + 1) {
            acc += *v as f64;
            n += 1;
        }
        for x in 0..w {
            tmp[y * w + x] = (acc / n as f64) as f32;
            if x + r + 1 < w {
                acc += row[x + r + 1] as f64;
                n += 1;
            }
            if x >= r {
                acc -= row[x - r] as f64;
                n -= 1;
            }
        }
    }
    let mut out = vec![0.0f32; w * h];
    for x in 0..w {
        let mut acc = 0.0f64;
        let mut n = 0usize;
        for y in 0..=r.min(h - 1) {
            acc += tmp[y * w + x] as f64;
            n += 1;
        }
        for y in 0..h {
            out[y * w + x] = (acc / n as f64) as f32;
            if y + r + 1 < h {
                acc += tmp[(y + r + 1) * w + x] as f64;
                n += 1;
            }
            if y >= r {
                acc -= tmp[(y - r) * w + x] as f64;
                n -= 1;
            }
        }
    }
    out
}

/// Colour guided filter (He et al.) of `p` guided by RGB image `guide`.
pub fn guided_filter(guide: &[[f32; 3]], p: &[f32], w: usize, h: usize, r: usize, eps: f32) -> Vec<f32> {
    let n = w * h;
    let ch = |k: usize| guide.iter().map(|c| c[k]).collect::<Vec<_>>();
    let (ir, ig, ib) = (ch(0), ch(1), ch(2));
    let prod = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).collect::<Vec<_>>();
    let bm = |v: &[f32]| box_mean(v, w, h, r);

    let (mr, mg, mb, mp) = (bm(&ir), bm(&ig), bm(&ib), bm(p));
    let (mrp, mgp, mbp) = (bm(&prod(&ir, p)), bm(&prod(&ig, p)), bm(&prod(&ib, p)));
    let (mrr, mrg, mrb) = (bm(&prod(&ir, &ir)), bm(&prod(&ir, &ig)), bm(&prod(&ir, &ib)));
    let (mgg, mgb, mbb) = (bm(&prod(&ig, &ig)), bm(&prod(&ig, &ib)), bm(&prod(&ib, &ib)));

    let mut a = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    let mut b = vec![0.0f32; n];
    for i in 0..n {
        let cov = [mrp[i] - mr[i] * mp[i], mgp[i] - mg[i] * mp[i], mbp[i] - mb[i] * mp[i]];
        // Symmetric covariance of the guide plus regularisation.
        let s00 = mrr[i] - mr[i] * mr[i] + eps;
        let s01 = mrg[i] - mr[i] * mg[i];
        let s02 = mrb[i] - mr[i] * mb[i];
        let s11 = mgg[i] - mg[i] * mg[i] + eps;
        let s12 = mgb[i] - mg[i] * mb[i];
        let s22 = mbb[i] - mb[i] * mb[i] + eps;
        // Inverse via the adjugate.
        let i00 = s11 * s22 - s12 * s12;
        let i01 = s02 * s12 - s01 * s22;
        let i02 = s01 * s12 - s02 * s11;
        let i11 = s00 * s22 - s02 * s02;
        let i12 = s01 * s02 - s00 * s12;
        let i22 = s00 * s11 - s01 * s01;
        let det = s00 * i00 + s01 * i01 + s02 * i02;
        let k = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };
        let ar = (i00 * cov[0] + i01 * cov[1] + i02 * cov[2]) * k;
        let ag = (i01 * cov[0] + i11 * cov[1] + i12 * cov[2]) * k;
        let ab = (i02 * cov[0] + i12 * cov[1] + i22 * cov[2]) * k;
        a[0][i] = ar;
        a[1][i] = ag;
        a[2][i] = ab;
        b[i] = mp[i] - ar * mr[i] - ag * mg[i] - ab * mb[i];
    }
    let (ma0, ma1, ma2, mbq) = (bm(&a[0]), bm(&a[1]), bm(&a[2]), bm(&b));
    (0..n).map(|i| ma0[i] * ir[i] + ma1[i] * ig[i] + ma2[i] * ib[i] + mbq[i]).collect()
}

/// Sharpen a soft matte against the image. `guide` is sRGB (perceptual
/// edges), `alpha` the upsampled network matte; only the uncertain band
/// around the edge is changed.
pub fn refine_alpha(guide: &[[f32; 3]], alpha: &[f32], w: usize, h: usize, px_per_mm: f32) -> Vec<f32> {
    // The upsampled network matte has a wide soft halo. Tighten it first so
    // the halo goes to 0/1, then let the guided filter re-introduce softness
    // only where the image itself has edges (hair strands).
    let tight: Vec<f32> = alpha.iter().map(|&a| smoothstep(0.25, 0.75, a)).collect();
    let r = ((px_per_mm * 0.25).round() as usize).clamp(2, 12);
    let refined = guided_filter(guide, &tight, w, h, r, 1e-3);
    // Band: anywhere within ~r of the soft edge of the original matte.
    let uncertain: Vec<f32> = alpha.iter().map(|&a| if a > 0.05 && a < 0.95 { 1.0 } else { 0.0 }).collect();
    let band = box_mean(&uncertain, w, h, r);
    tight
        .iter()
        .zip(&refined)
        .zip(&band)
        .map(|((&a, &q), &bd)| {
            let t = (bd * 4.0).min(1.0);
            (a + (q.clamp(0.0, 1.0) - a) * t).clamp(0.0, 1.0)
        })
        .collect()
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Local colour of the original background behind each pixel: a normalised
/// blur that only counts clearly-background pixels.
pub fn local_background(img: &[[f32; 3]], alpha: &[f32], w: usize, h: usize, r: usize) -> Vec<[f32; 3]> {
    let wt: Vec<f32> = alpha.iter().map(|&a| (1.0 - a).powi(4)).collect();
    let sw = box_mean(&wt, w, h, r);
    let ch = |k: usize| box_mean(&img.iter().zip(&wt).map(|(c, &x)| c[k] * x).collect::<Vec<_>>(), w, h, r);
    let (sr, sg, sb) = (ch(0), ch(1), ch(2));
    (0..w * h).map(|i| if sw[i] > 1e-4 { [sr[i] / sw[i], sg[i] / sw[i], sb[i] / sw[i]] } else { img[i] }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mean_of_constant_is_constant() {
        let v = vec![2.0; 7 * 5];
        assert!(box_mean(&v, 7, 5, 3).iter().all(|&x| (x - 2.0).abs() < 1e-6));
    }

    #[test]
    fn guided_filter_snaps_soft_step_to_image_edge() {
        // Image: dark left half, bright right half. Matte: soft ramp, centred off the edge.
        let (w, h) = (40, 8);
        let guide: Vec<[f32; 3]> = (0..w * h).map(|i| if i % w < 20 { [0.1; 3] } else { [0.9; 3] }).collect();
        let alpha: Vec<f32> = (0..w * h).map(|i| ((i % w) as f32 - 14.0) / 10.0).map(|v| v.clamp(0.0, 1.0)).collect();
        let q = refine_alpha(&guide, &alpha, w, h, 10.0);
        let row = &q[4 * w..5 * w];
        assert!(row[17] < 0.15, "left of edge: {}", row[17]);
        assert!(row[22] > 0.85, "right of edge: {}", row[22]);
    }
}
