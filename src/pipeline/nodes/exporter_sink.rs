//! ExporterSink node — exports data to CSV/JSON/binary files.
//!
//! Built from `DataPersistenceConfig`.

use crate::pipeline::id::VarId;
use crate::pipeline::node::NodeContext;
use crate::pipeline::packet::ConfigValue;
use crate::pipeline::port::{PortDescriptor, PortDirection, PortKind};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

static PORTS: &[PortDescriptor] = &[PortDescriptor {
    name: "in",
    direction: PortDirection::Input,
    kind: PortKind::DataStream,
}];

/// Export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
}

/// Export layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLayout {
    /// One row per sample: timestamp, var_id, raw, converted
    Long,
    /// One row per tick: timestamp, var1, var2, ...
    Wide,
}

/// Which value to use for a variable in wide-format export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueChoice {
    Raw,
    Converted,
}

/// ExporterSink: writes data to a file in the configured format.
pub struct ExporterSinkNode {
    writer: Option<BufWriter<File>>,
    path: Option<PathBuf>,
    format: ExportFormat,
    active: bool,
    header_written: bool,
    rows_written: u64,
    layout: ExportLayout,
    /// Per-variable value choice for wide layout. Key: VarId raw u32.
    /// Variables not in this map default to Converted.
    value_choices: HashMap<u32, ValueChoice>,
    /// Ordered list of variable IDs for wide-format columns.
    /// Built from var_tree on first data tick. Determines column order.
    wide_columns: Vec<(VarId, String)>,
}

impl ExporterSinkNode {
    pub fn new() -> Self {
        Self {
            writer: None,
            path: None,
            format: ExportFormat::Csv,
            active: false,
            header_written: false,
            rows_written: 0,
            layout: ExportLayout::Long,
            value_choices: HashMap::new(),
            wide_columns: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        "ExporterSink"
    }

    pub fn ports(&self) -> &[PortDescriptor] {
        PORTS
    }

    pub fn on_activate(&mut self, _ctx: &mut NodeContext) {
        if self.active {
            self.open_file();
        }
    }

    pub fn on_data(&mut self, ctx: &mut NodeContext) {
        if !self.active || ctx.input.is_empty() {
            return;
        }

        let Some(ref mut writer) = self.writer else {
            return;
        };

        match self.format {
            ExportFormat::Csv => match self.layout {
                ExportLayout::Long => {
                    if !self.header_written {
                        let _ = writeln!(writer, "timestamp_s,var_id,raw,converted");
                        self.header_written = true;
                    }
                    let ts = ctx.input.timestamp.as_secs_f64();
                    for sample in ctx.input.iter() {
                        let _ = writeln!(
                            writer,
                            "{:.6},{},{:.10},{:.10}",
                            ts, sample.var_id.0, sample.raw, sample.converted
                        );
                    }
                    self.rows_written += ctx.input.len() as u64;
                }
                ExportLayout::Wide => {
                    if !self.header_written {
                        // Build column list from var_tree enabled leaves
                        self.wide_columns = ctx
                            .var_tree
                            .enabled_leaves()
                            .map(|node| (node.id, node.name.clone()))
                            .collect();

                        // Write header
                        let mut header = String::from("timestamp_s");
                        for (_id, name) in &self.wide_columns {
                            header.push(',');
                            header.push_str(name);
                        }
                        let _ = writeln!(writer, "{}", header);
                        self.header_written = true;
                    }

                    // Build lookup from this tick's samples
                    let mut sample_map: HashMap<u32, &crate::pipeline::packet::Sample> =
                        HashMap::new();
                    for sample in ctx.input.iter() {
                        sample_map.insert(sample.var_id.0, sample);
                    }

                    // Write one row
                    let ts = ctx.input.timestamp.as_secs_f64();
                    let mut row = format!("{:.6}", ts);
                    for (id, _name) in &self.wide_columns {
                        row.push(',');
                        if let Some(sample) = sample_map.get(&id.0) {
                            let choice = self
                                .value_choices
                                .get(&id.0)
                                .copied()
                                .unwrap_or(ValueChoice::Converted);
                            let val = match choice {
                                ValueChoice::Raw => sample.raw,
                                ValueChoice::Converted => sample.converted,
                            };
                            use std::fmt::Write as FmtWrite;
                            let _ = write!(row, "{:.10}", val);
                        }
                        // Missing variable → empty cell (comma already pushed)
                    }
                    let _ = writeln!(writer, "{}", row);
                    self.rows_written += 1;
                }
            },
            ExportFormat::Json => {
                let ts = ctx.input.timestamp.as_secs_f64();
                for sample in ctx.input.iter() {
                    let _ = writeln!(
                        writer,
                        r#"{{"t":{:.6},"id":{},"raw":{:.10},"val":{:.10}}}"#,
                        ts, sample.var_id.0, sample.raw, sample.converted
                    );
                }
                self.rows_written += ctx.input.len() as u64;
            }
        }
    }

    pub fn on_deactivate(&mut self, _ctx: &mut NodeContext) {
        self.close_file();
    }

    pub fn on_config_change(&mut self, key: &str, value: &ConfigValue, _ctx: &mut NodeContext) {
        match key {
            "path" => {
                if let Some(p) = value.as_str() {
                    self.path = Some(PathBuf::from(p));
                }
            }
            "format" => {
                if let Some(f) = value.as_str() {
                    self.format = match f {
                        "json" => ExportFormat::Json,
                        _ => ExportFormat::Csv,
                    };
                }
            }
            "layout" => {
                if let Some(l) = value.as_str() {
                    self.layout = match l {
                        "wide" => ExportLayout::Wide,
                        _ => ExportLayout::Long,
                    };
                }
            }
            "value_choices" => {
                // Format: "0:raw,3:raw,7:converted" — var_id:choice pairs.
                // Variables not listed default to Converted.
                if let Some(s) = value.as_str() {
                    self.value_choices.clear();
                    if !s.is_empty() {
                        for pair in s.split(',') {
                            let pair = pair.trim();
                            if let Some((id_str, choice_str)) = pair.split_once(':') {
                                if let Ok(id) = id_str.trim().parse::<u32>() {
                                    let choice = match choice_str.trim() {
                                        "raw" => ValueChoice::Raw,
                                        _ => ValueChoice::Converted,
                                    };
                                    self.value_choices.insert(id, choice);
                                }
                            }
                        }
                    }
                }
            }
            "start" => {
                self.active = true;
                self.wide_columns.clear();
                self.open_file();
            }
            "stop" => {
                self.active = false;
                self.close_file();
            }
            _ => {}
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn rows_written(&self) -> u64 {
        self.rows_written
    }

    fn open_file(&mut self) {
        if let Some(ref path) = self.path {
            match File::create(path) {
                Ok(f) => {
                    self.writer = Some(BufWriter::new(f));
                    self.header_written = false;
                    self.rows_written = 0;
                    self.wide_columns.clear();
                    tracing::info!("ExporterSink opened file: {:?}", path);
                }
                Err(e) => {
                    tracing::error!("ExporterSink failed to open file {:?}: {}", path, e);
                }
            }
        }
    }

    fn close_file(&mut self) {
        if let Some(ref mut writer) = self.writer {
            let _ = writer.flush();
        }
        self.writer = None;
        if self.rows_written > 0 {
            tracing::info!(
                "ExporterSink closed file after {} rows",
                self.rows_written
            );
        }
    }
}
