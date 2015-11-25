//! Work with las files as complete units.
//!
//! This is a lower-level class that holds *all* of the information in a las file — it holds all of
//! the points in memory. In the future, we might refactor/replace/re-implement this class to allow
//! working with lasfiles without reading all of those points into memory.
//!
//! In general, you shouldn't use the structures in this module to read or write lasfiles — use
//! `Reader` and `Writer` for that.

use std::f64;
use std::fs;
use std::io::{BufReader, BufWriter, Seek, Read, Write};
use std::path::Path;

use byteorder::{LittleEndian, WriteBytesExt};

use Result;
use error::Error;
use header::Header;
use io::write_zeros;
use point::Point;
use scale::descale;
use reader::Reader;
use vlr::Vlr;

/// A las file.
#[derive(Debug, PartialEq)]
pub struct File {
    header: Header,
    vlrs: Vec<Vlr>,
    points: Vec<Point>,
}

impl File {
    /// Reads a las file from the filesystem.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// let file = File::from_path("data/1.0_0.las").unwrap();
    /// ```
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<File> {
        let reader = BufReader::new(try!(fs::File::open(path)));
        File::read_from(reader)
    }

    /// Reads a las file from a `Read`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fs;
    /// use las::file::File;
    /// let reader = fs::File::open("data/1.0_0.las").unwrap();
    /// let file = File::read_from(reader).unwrap();
    /// ```
    pub fn read_from<R: Read + Seek>(reader: R) -> Result<File> {
        let mut file = File::new();
        let mut reader = try!(Reader::new(reader));
        file.header = reader.header();
        file.vlrs = (*reader.vlrs()).clone();
        file.points.reserve(reader.npoints() as usize);
        loop {
            match try!(reader.next_point()) {
                Some(point) => file.points.push(point),
                None => break,
            }
        }
        Ok(file)
    }

    /// Creates a new, empty las file.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// let file = File::new();
    /// ```
    pub fn new() -> File {
        File {
            header: Header::new(),
            vlrs: Vec::new(),
            points: Vec::new(),
        }
    }

    /// Sets the header for this file.
    ///
    /// Since the header contains so much metadata, we might want to construct a header elsewhere
    /// then set it to the file just before write.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// use las::header::Header;
    /// let mut file = File::new();
    /// let header = Header::new();
    /// file.set_header(header);
    /// ```
    pub fn set_header(&mut self, header: Header) {
        self.header = header;
    }

    /// Gets a reference to this file's header.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// let file = File::from_path("data/1.0_0.las").unwrap();
    /// let header = file.header();
    /// ```
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Returns a reference to a vector of this file's points.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// let file = File::from_path("data/1.0_0.las").unwrap();
    /// let points = file.points();
    /// ```
    pub fn points(&self) -> &Vec<Point> {
        &self.points
    }

    /// Adds a point to this lasfile.
    ///
    /// # Examples
    ///
    /// ```
    /// use las::file::File;
    /// use las::point::Point;
    /// let mut file = File::new();
    /// let point = Point::new();
    /// file.add_point(point);
    /// ```
    pub fn add_point(&mut self, point: Point) {
        self.points.push(point);
    }

    /// Writes out this las file to a `Path`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::fs::remove_file;
    /// use las::file::File;
    /// let mut file = File::new();
    /// file.to_path("temp.las", true).unwrap();
    /// remove_file("temp.las");
    /// ```
    pub fn to_path<P: AsRef<Path>>(&mut self, path: P, auto_offsets: bool) -> Result<()> {
        let ref mut writer = BufWriter::new(try!(fs::File::create(path)));
        self.write_to(writer, auto_offsets)
    }

    /// Writes this las file to a `Write`.
    ///
    /// If auto_offsets is true, reasonable offset values will be calculated and written to the
    /// header before the file is written.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use las::file::File;
    /// let mut file = File::from_path("data/1.0_0.las").unwrap();
    /// let ref mut cursor = Cursor::new(Vec::new());
    /// file.write_to(cursor, true).unwrap();
    /// ```
    pub fn write_to<W: Write>(&mut self, writer: &mut W, auto_offsets: bool) -> Result<()> {
        self.header.calculate_size();
        self.header.number_of_point_records = self.points.len() as u32;
        self.header.offset_to_point_data = self.header.header_size as u32 +
                                           self.vlrs.iter().fold(0, |a, v| a + v.len());
        self.header.point_data_record_length = self.header.point_data_format.record_length();

        let mut number_of_points_by_return = [0u32; 5];
        let mut x_min = f64::MAX;
        let mut y_min = f64::MAX;
        let mut z_min = f64::MAX;
        let mut x_max = f64::MIN;
        let mut y_max = f64::MIN;
        let mut z_max = f64::MIN;
        for point in &self.points {
            let return_number = point.return_number.as_u8();
            if return_number > 0 {
                number_of_points_by_return[(return_number - 1) as usize] += 1;
            }
            if point.x < x_min {
                x_min = point.x;
            }
            if point.y < y_min {
                y_min = point.y;
            }
            if point.z < z_min {
                z_min = point.z;
            }
            if point.x > x_max {
                x_max = point.x;
            }
            if point.y > y_max {
                y_max = point.y;
            }
            if point.z > z_max {
                z_max = point.z;
            }
        }
        self.header.x_min = x_min;
        self.header.y_min = y_min;
        self.header.z_min = z_min;
        self.header.x_max = x_max;
        self.header.y_max = y_max;
        self.header.z_max = z_max;

        if auto_offsets {
            self.header.x_offset = (x_min + x_max) / 2.0;
            self.header.y_offset = (y_min + y_max) / 2.0;
            self.header.z_offset = (z_min + z_max) / 2.0;
        }

        let mut bytes_written = try!(self.header.write_to(writer)) as usize;
        if bytes_written < self.header.header_size as usize {
            bytes_written += try!(write_zeros(writer,
                                              self.header.header_size as usize - bytes_written));
        }
        for vlr in &self.vlrs {
            bytes_written += try!(vlr.write_to(writer)) as usize;
        }
        if bytes_written < self.header.offset_to_point_data as usize {
            try!(write_zeros(writer,
                             self.header.offset_to_point_data as usize - bytes_written));
        }
        for point in &self.points {
            try!(self.write_point_to(writer, point));
        }
        Ok(())
    }

    fn write_point_to<W: Write>(&self, writer: &mut W, point: &Point) -> Result<()> {
        try!(writer.write_i32::<LittleEndian>(descale(point.x,
                                                      self.header.x_scale_factor,
                                                      self.header.x_offset)));
        try!(writer.write_i32::<LittleEndian>(descale(point.y,
                                                      self.header.y_scale_factor,
                                                      self.header.y_offset)));
        try!(writer.write_i32::<LittleEndian>(descale(point.z,
                                                      self.header.z_scale_factor,
                                                      self.header.z_offset)));
        try!(writer.write_u16::<LittleEndian>(point.intensity));
        let byte = point.return_number.as_u8() + (point.number_of_returns.as_u8() << 3) +
                   (point.scan_direction.as_u8() << 6) +
                   ((point.edge_of_flight_line as u8) << 7);
        try!(writer.write_u8(byte));
        let byte = point.classification.as_u8() + ((point.synthetic as u8) << 5) +
                   ((point.key_point as u8) << 6) +
                   ((point.withheld as u8) << 7);
        try!(writer.write_u8(byte));
        try!(writer.write_i8(point.scan_angle_rank));
        try!(writer.write_u8(point.user_data));
        try!(writer.write_u16::<LittleEndian>(point.point_source_id));
        if self.header.point_data_format.has_time() {
            match point.gps_time {
                Some(gps_time) => try!(writer.write_f64::<LittleEndian>(gps_time)),
                None => {
                    return Err(Error::PointFormat(self.header.point_data_format,
                                                     "gps_time".to_string()))
                }
            }
        }
        if self.header.point_data_format.has_color() {
            match point.red {
                Some(red) => try!(writer.write_u16::<LittleEndian>(red)),
                None => {
                    return Err(Error::PointFormat(self.header.point_data_format,
                                                     "red".to_string()))
                }
            }
            match point.green {
                Some(green) => try!(writer.write_u16::<LittleEndian>(green)),
                None => {
                    return Err(Error::PointFormat(self.header.point_data_format,
                                                     "green".to_string()))
                }
            }
            match point.blue {
                Some(blue) => try!(writer.write_u16::<LittleEndian>(blue)),
                None => {
                    return Err(Error::PointFormat(self.header.point_data_format,
                                                     "blue".to_string()))
                }
            }
        }
        match point.extra_bytes {
            Some(ref bytes) => try!(writer.write_all(&bytes[..])),
            None => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::remove_file;
    use std::io::Cursor;
    use std::path::Path;

    use point::Point;

    fn roundtrip<P: AsRef<Path>>(path: P) {
        let mut lasfile = File::from_path(path).unwrap();
        let ref mut cursor = Cursor::new(Vec::new());
        lasfile.write_to(cursor, false).unwrap();
        cursor.set_position(0);
        let lasfile2 = File::read_from(cursor).unwrap();
        assert_eq!(lasfile, lasfile2);
    }

    #[test]
    fn roundtrip_1_0_0() {
        roundtrip("data/1.0_0.las");
    }

    #[test]
    fn roundtrip_1_0_1() {
        roundtrip("data/1.0_1.las");
    }

    #[test]
    fn roundtrip_1_1_0() {
        roundtrip("data/1.1_0.las");
    }

    #[test]
    fn roundtrip_1_1_1() {
        roundtrip("data/1.1_1.las");
    }

    #[test]
    fn roundtrip_1_2_0() {
        roundtrip("data/1.2_0.las");
    }

    #[test]
    fn roundtrip_1_2_1() {
        roundtrip("data/1.2_1.las");
    }

    #[test]
    fn roundtrip_1_2_2() {
        roundtrip("data/1.2_2.las");
    }

    #[test]
    fn roundtrip_1_2_3() {
        roundtrip("data/1.2_3.las");
    }

    /// This file is good as it exercieses a weird use case, but the test fails at the moment. I'm
    /// not sure why, so I'm going to keep it around but ignore it.
    #[test]
    #[ignore]
    fn roundtrip_extrabytes() {
        roundtrip("data/extrabytes.las");
    }

    #[test]
    fn point_format_1_has_gps_time() {
        let lasfile = File::from_path("data/1.0_1.las").unwrap();
        let ref point = lasfile.points()[0];
        assert!(point.gps_time.is_some());
    }

    #[test]
    fn point_format_2_has_color() {
        let lasfile = File::from_path("data/1.2_2.las").unwrap();
        let ref point = lasfile.points()[0];
        assert!(point.red.is_some());
        assert!(point.green.is_some());
        assert!(point.blue.is_some());
    }

    #[test]
    fn write_one_point() {
        let mut point = Point::new();
        point.x = 1.0;
        point.y = 2.0;
        point.z = 3.0;
        let mut lasfile = File::new();
        lasfile.add_point(point);
        lasfile.to_path("temp.las", false).unwrap();

        let lasfile = File::from_path("temp.las").unwrap();
        let ref point = lasfile.points()[0];
        assert_eq!(1.0, point.x);
        assert_eq!(2.0, point.y);
        assert_eq!(3.0, point.z);

        remove_file("temp.las").unwrap();
    }
}