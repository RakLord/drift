//! A borrowed view of a row-major weight matrix with the bias stored as an
//! extra last column, so each row is one neuron: `[w_0 .. w_n-1, bias]`.

pub struct AffineRows<'a> {
    pub data: &'a [f32],
    pub rows: usize,
    /// Inputs per row (the bias column is extra).
    pub inputs: usize,
}

impl AffineRows<'_> {
    pub fn row(&self, r: usize) -> &[f32] {
        let width = self.inputs + 1;
        &self.data[r * width..(r + 1) * width]
    }

    /// `out[r] = row_r . input + bias_r`.
    pub fn apply(&self, input: &[f32], out: &mut [f32]) {
        debug_assert_eq!(input.len(), self.inputs);
        debug_assert_eq!(out.len(), self.rows);
        for (r, o) in out.iter_mut().enumerate() {
            let row = self.row(r);
            let (weights, bias) = row.split_at(self.inputs);
            *o = weights.iter().zip(input).map(|(w, x)| w * x).sum::<f32>() + bias[0];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_rows() {
        // Two neurons over two inputs.
        let data = [1.0, 2.0, 0.5, -1.0, 0.0, 3.0];
        let m = AffineRows { data: &data, rows: 2, inputs: 2 };
        let mut out = [0.0; 2];
        m.apply(&[2.0, 1.0], &mut out);
        assert_eq!(out, [1.0 * 2.0 + 2.0 * 1.0 + 0.5, -2.0 + 3.0]);
    }
}
