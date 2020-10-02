use std::convert::TryInto;
use std::io::Read;

use structopt::StructOpt;

#[derive(StructOpt)]
struct PmReader {
    /// Path to serial port to use. On Linux this is something like
    /// `/dev/ttyUSB0`. On Mac, `/dev/tty.somethingorother`.
    serial_port: std::path::PathBuf,
}

fn main() {
    let args = PmReader::from_args();
    let mut port = serialport::open(&args.serial_port).unwrap();
    // Device produces output at 1Hz; if we haven't heard anything for 2
    // seconds, something is wrong.
    port.set_timeout(std::time::Duration::from_secs(2)).unwrap();

    let mut buffer = [0; 10];
    loop {
        // Read until we hit the synchronization header byte.
        port.read_exact(&mut buffer[..1]).unwrap();
        if buffer[0] != 0xAA {
            continue;
        }
        // To avoid false positives in data, check for the expected next byte
        // too.
        port.read_exact(&mut buffer[..1]).unwrap();
        if buffer[0] != 0xC0 {
            continue;
        }

        // Now, read the rest of the packet (note: overwrites the first bytes
        // read) and check that the *trailing* byte is correct.
        port.read_exact(&mut buffer[..8]).unwrap();
        if buffer[7] != 0xAB {
            continue;
        }

        // We're now pretty sure we're synchronized, interpret the bytes. Both
        // PM values are sent as little-endian 16-bit values measured in tenths
        // of a microgram per cubic meter. Divide by 10 to get SI units.
        let pm2_5 = u16::from_le_bytes(buffer[0..2].try_into().unwrap()) as f64 / 10.;
        let pm10 = u16::from_le_bytes(buffer[2..4].try_into().unwrap()) as f64 / 10.;
        // CSV output.
        println!(
            "{},{},{:.3?},{},{:.3?},{:.3?}",
            chrono::Local::now().to_rfc3339(),
            pm2_5,
            aqi(PM25_AQI, pm2_5).unwrap_or(501.),
            pm10,
            aqi(PM10_AQI, pm10).unwrap_or(501.),
            aqi(PM25_AQI, lrapa(pm2_5)).unwrap_or(501.),
        );
    }
}

/// AQI curve parameters for PM2.5.
const PM25_AQI: &[(f64, f64, f64)] = &[
    (12.0, 0., 50.),
    (35.4, 51., 100.),
    (55.4, 101., 150.),
    (150.4, 151., 200.),
    (250.4, 201., 300.),
    (500.4, 301., 500.),
];

/// AQI curve parameters for PM10.
const PM10_AQI: &[(f64, f64, f64)] = &[
    (54.0, 0., 50.),
    (154., 51., 100.),
    (254., 101., 150.),
    (354., 151., 200.),
    (424., 201., 300.),
    (604., 301., 500.),
];

/// Converts a particulate concentration in micrograms per cubic meter (`conc`)
/// into an AQI number using `table`.
fn aqi(table: &[(f64, f64, f64)], conc: f64) -> Option<f64> {
    let mut conc_lo = 0.;
    for entry in table {
        if conc <= entry.0 {
            let (conc_hi, aqi_lo, aqi_hi) = entry;
            return Some(((aqi_hi - aqi_lo) / (conc_hi - conc_lo)) * (conc - conc_lo) + aqi_lo);
        } else {
            conc_lo = entry.0 + 0.1;
        }
    }
    // Values off the top end of the table are not defined in AQI.
    None
}

/// Apply LRAPA correction.
fn lrapa(conc: f64) -> f64 {
    conc * 0.5 - 0.66
}
