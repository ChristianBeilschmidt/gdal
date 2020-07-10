use crate::gdal_common::gdal_major_object::MajorObject;
use crate::gdal_common::metadata::Metadata;
use crate::raster::driver::_register_drivers;
use crate::raster::types::GdalType;
use crate::raster::{Driver, DriverExt, RasterBand, RasterBandExt};
use crate::utils::{_last_cpl_err, _last_null_pointer_err, _string};
use gdal_sys::{self, CPLErr, GDALAccess, GDALDataType, GDALDatasetH, GDALMajorObjectH};
use libc::{c_double, c_int};
use std::ffi::CString;
use std::path::Path;
use std::ptr::null_mut;

#[cfg(feature = "ndarray")]
use ndarray::Array2;

use crate::errors::*;

pub type GeoTransform = [c_double; 6];

pub struct Dataset {
    c_dataset: GDALDatasetH,
}

impl MajorObject for Dataset {
    unsafe fn gdal_object_ptr(&self) -> GDALMajorObjectH {
        self.c_dataset
    }
}

impl Metadata for Dataset {}

impl Drop for Dataset {
    fn drop(&mut self) {
        unsafe {
            gdal_sys::GDALClose(self.c_dataset);
        }
    }
}

pub trait DatasetExt: AsRef<Dataset> {

    fn c_dataset(&self) -> GDALDatasetH;

    fn open(path: &Path) -> Result<Dataset> {
        _register_drivers();
        let filename = path.to_string_lossy();
        let c_filename = CString::new(filename.as_ref())?;
        let c_dataset = unsafe { gdal_sys::GDALOpen(c_filename.as_ptr(), GDALAccess::GA_ReadOnly) };
        if c_dataset.is_null() {
            Err(_last_null_pointer_err("GDALOpen"))?;
        }
        Ok(Dataset { c_dataset })
    }

    unsafe fn from_c_ptr(c_dataset: GDALDatasetH) -> Dataset {
        Dataset { c_dataset }
    }

    fn rasterband(&self, band_index: isize) -> Result<RasterBand> {
        unsafe {
            let c_band = gdal_sys::GDALGetRasterBand(self.c_dataset(), band_index as c_int);
            if c_band.is_null() {
                Err(_last_null_pointer_err("GDALGetRasterBand"))?;
            }
            Ok(RasterBand::from_c_ptr(c_band, self.as_ref()))
        }
    }

    fn size(&self) -> (usize, usize) {
        let size_x = unsafe { gdal_sys::GDALGetRasterXSize(self.c_dataset()) } as usize;
        let size_y = unsafe { gdal_sys::GDALGetRasterYSize(self.c_dataset()) } as usize;
        (size_x, size_y)
    }

    /// Get block size from a 'Dataset'.
    /// # Arguments
    /// * band_index - the band_index
    /*
    pub fn size_block(&self, band_index: isize) -> (usize, usize) {
        let band = self.rasterband(band_index)?;
        band.size_block()
    }
    */

    fn driver(&self) -> Driver {
        unsafe {
            let c_driver = gdal_sys::GDALGetDatasetDriver(self.c_dataset());
            Driver::from_c_ptr(c_driver)
        }
    }

    fn count(&self) -> isize {
        (unsafe { gdal_sys::GDALGetRasterCount(self.c_dataset()) }) as isize
    }

    fn projection(&self) -> String {
        let rv = unsafe { gdal_sys::GDALGetProjectionRef(self.c_dataset()) };
        _string(rv)
    }

    fn set_projection(&self, projection: &str) -> Result<()> {
        let c_projection = CString::new(projection)?;
        unsafe { gdal_sys::GDALSetProjection(self.c_dataset(), c_projection.as_ptr()) };
        Ok(())
    }

    /// Affine transformation called geotransformation.
    ///
    /// This is like a linear transformation preserves points, straight lines and planes.
    /// Also, sets of parallel lines remain parallel after an affine transformation.
    /// # Arguments
    /// * transformation - coeficients of transformations
    ///
    /// x-coordinate of the top-left corner pixel (x-offset)
    /// width of a pixel (x-resolution)
    /// row rotation (typically zero)
    /// y-coordinate of the top-left corner pixel
    /// column rotation (typically zero)
    /// height of a pixel (y-resolution, typically negative)
    fn set_geo_transform(&self, transformation: &GeoTransform) -> Result<()> {
        assert_eq!(transformation.len(), 6);
        let rv = unsafe {
            gdal_sys::GDALSetGeoTransform(self.c_dataset(), transformation.as_ptr() as *mut f64)
        };
        if rv != CPLErr::CE_None {
            Err(_last_cpl_err(rv))?;
        }
        Ok(())
    }

    /// Get affine transformation coefficients.
    ///
    /// x-coordinate of the top-left corner pixel (x-offset)
    /// width of a pixel (x-resolution)
    /// row rotation (typically zero)
    /// y-coordinate of the top-left corner pixel
    /// column rotation (typically zero)
    /// height of a pixel (y-resolution, typically negative)
    fn geo_transform(&self) -> Result<GeoTransform> {
        let mut transformation = GeoTransform::default();
        let rv =
            unsafe { gdal_sys::GDALGetGeoTransform(self.c_dataset(), transformation.as_mut_ptr()) };

        // check if the dataset has a GeoTransform
        if rv != CPLErr::CE_None {
            Err(_last_cpl_err(rv))?;
        }
        Ok(transformation)
    }

    fn create_copy(&self, driver: &Driver, filename: &str) -> Result<Dataset> {
        let c_filename = CString::new(filename)?;
        let c_dataset = unsafe {
            gdal_sys::GDALCreateCopy(
                driver.c_driver(),
                c_filename.as_ptr(),
                self.c_dataset(),
                0,
                null_mut(),
                None,
                null_mut(),
            )
        };
        if c_dataset.is_null() {
            Err(_last_null_pointer_err("GDALCreateCopy"))?;
        }
        Ok(Dataset { c_dataset })
    }

    fn band_type(&self, band_index: isize) -> Result<GDALDataType::Type> {
        self.rasterband(band_index).map(|band| band.band_type())
    }

    /// Read a 'Buffer<u8>' from a 'Dataset'.
    /// # Arguments
    /// * band_index - the band_index
    /// * window - the window position from top left
    /// * window_size - the window size (GDAL will interpolate data if window_size != buffer_size)
    /// * buffer_size - the desired size of the 'Buffer'
    fn read_raster(
        &self,
        band_index: isize,
        window: (isize, isize),
        window_size: (usize, usize),
        size: (usize, usize),
    ) -> Result<ByteBuffer> {
        self.read_raster_as::<u8>(band_index, window, window_size, size)
    }

    /// Read a full 'Dataset' as 'Buffer<T>'.
    /// # Arguments
    /// * band_index - the band_index
    fn read_full_raster_as<T: Copy + GdalType>(&self, band_index: isize) -> Result<Buffer<T>> {
        self.rasterband(band_index)?.read_band_as()
    }

    /// Read a 'Buffer<T>' from a 'Dataset'. T implements 'GdalType'
    /// # Arguments
    /// * band_index - the band_index
    /// * window - the window position from top left
    /// * window_size - the window size (GDAL will interpolate data if window_size != buffer_size)
    /// * buffer_size - the desired size of the 'Buffer'
    fn read_raster_as<T: Copy + GdalType>(
        &self,
        band_index: isize,
        window: (isize, isize),
        window_size: (usize, usize),
        size: (usize, usize),
    ) -> Result<Buffer<T>> {
        self.rasterband(band_index)?
            .read_as(window, window_size, size)
    }

    #[cfg(feature = "ndarray")]
    /// Read a 'Array2<T>' from a 'Dataset'. T implements 'GdalType'.
    /// # Arguments
    /// * band_index - the band_index
    /// * window - the window position from top left
    /// * window_size - the window size (GDAL will interpolate data if window_size != array_size)
    /// * array_size - the desired size of the 'Array'
    fn read_as_array<T: Copy + GdalType>(
        &self,
        band_index: isize,
        window: (isize, isize),
        window_size: (usize, usize),
        array_size: (usize, usize),
    ) -> Result<Array2<T>> {
        self.rasterband(band_index)?
            .read_as_array(window, window_size, array_size)
    }

    /// Write a 'Buffer<T>' into a 'Dataset'.
    /// # Arguments
    /// * band_index - the band_index
    /// * window - the window position from top left
    /// * window_size - the window size (GDAL will interpolate data if window_size != Buffer.size)
    fn write_raster<T: GdalType + Copy>(
        &self,
        band_index: isize,
        window: (isize, isize),
        window_size: (usize, usize),
        buffer: &Buffer<T>,
    ) -> Result<()> {
        self.rasterband(band_index)?
            .write(window, window_size, buffer)
    }
}

impl AsRef<Dataset> for Dataset {
    fn as_ref(&self) -> &Dataset {
        self
    }    
}

impl DatasetExt for Dataset {
    fn c_dataset(&self) -> GDALDatasetH {
        self.c_dataset
    }
    
}

pub struct Buffer<T: GdalType> {
    pub size: (usize, usize),
    pub data: Vec<T>,
}

impl<T: GdalType> Buffer<T> {
    pub fn new(size: (usize, usize), data: Vec<T>) -> Buffer<T> {
        Buffer { size, data }
    }
}

pub type ByteBuffer = Buffer<u8>;
