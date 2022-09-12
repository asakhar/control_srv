pub trait Lerpable {
  fn lerp(self, left: Self, right: Self) -> Self;
  fn inv_lerp(self, left: Self, right: Self) -> Self;
}

impl<T> Lerpable for T
where
  T: std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Add<Output = Self>
    + std::ops::Div<Output = Self>
    + Copy,
{
  fn lerp(self, left: Self, right: Self) -> Self {
    self * (right - left) + left
  }
  fn inv_lerp(self, left: Self, right: Self) -> Self {
    (self - left) / (right - left)
  }
}

pub trait WriteSizedExt {
  fn write_sized(&mut self, buf: &[u8]) -> std::io::Result<()>;
}

impl<T: std::io::Write> WriteSizedExt for T {
  fn write_sized(&mut self, buf: &[u8]) -> std::io::Result<()> {
    self.write_all(&buf.len().to_le_bytes())?;
    self.write_all(buf)
  }
}
pub trait ReadSizedExt {
  fn read_sized(&mut self) -> std::io::Result<Vec<u8>>;
}

impl<T: std::io::Read> ReadSizedExt for T {
  fn read_sized(&mut self) -> std::io::Result<Vec<u8>> {
    let mut buf = [0u8; std::mem::size_of::<usize>()];
    self.read_exact(&mut buf)?;
    let len = usize::from_le_bytes(buf);

    let mut v = vec![0u8; len];
    self.read_exact(&mut v)?;
    Ok(v)
  }
}
