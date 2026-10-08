// Day 1 skeleton: two panes, splitter, path entry, synchronous read_dir load.
slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::path::Path;
use std::rc::Rc;
use std::time::{Instant, UNIX_EPOCH};

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_time(secs: i64) -> String {
    // Civil date from Unix seconds (Howard Hinnant's algorithm), local tz ignored for the prototype.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

fn load_sync(path: &Path) -> (Vec<Row>, String) {
    let t0 = Instant::now();
    let mut rows = Vec::new();
    match std::fs::read_dir(path) {
        Ok(rd) => {
            for e in rd.flatten() {
                let md = e.metadata().ok();
                let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = md
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                rows.push(Row {
                    name: SharedString::from(e.file_name().to_string_lossy().as_ref()),
                    size: if is_dir { SharedString::default() } else { format_size(size).into() },
                    modified: format_time(modified).into(),
                    is_dir,
                });
            }
            let n = rows.len();
            (rows, format!("{n} items, loaded in {:?}", t0.elapsed()))
        }
        Err(err) => (rows, format!("error: {err}")),
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let left: Rc<VecModel<Row>> = Rc::new(VecModel::default());
    let right: Rc<VecModel<Row>> = Rc::new(VecModel::default());
    app.set_left_rows(ModelRc::from(left.clone()));
    app.set_right_rows(ModelRc::from(right.clone()));

    let start = std::env::args().nth(1).unwrap_or_else(|| "C:\\".to_string());
    app.set_left_path(start.clone().into());
    app.set_right_path(start.clone().into());
    let (rows, status) = load_sync(Path::new(&start));
    left.set_vec(rows);
    app.set_left_status(status.into());

    {
        let left = left.clone();
        let right = right.clone();
        let w = app.as_weak();
        app.on_navigate(move |pane, path| {
            let app = w.unwrap();
            let (rows, status) = load_sync(Path::new(path.as_str()));
            if pane == 0 {
                left.set_vec(rows);
                app.set_left_status(status.into());
            } else {
                right.set_vec(rows);
                app.set_right_status(status.into());
            }
        });
    }
    {
        let w = app.as_weak();
        app.on_toggle_split(move || {
            let app = w.unwrap();
            let now = !app.get_split();
            app.set_split(now);
            if now {
                let p = app.get_left_path();
                app.set_right_path(p.clone());
                app.invoke_navigate(1, p);
            }
        });
    }

    app.run()
}
