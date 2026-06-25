mod exporter;
mod parser;

pub use exporter::{Datasets2ExportReport, ExportDirectoryOptions, run_export_directory};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) const FIXTURE: &str = "./2017/2017-01-01/344397.xml
东\t0\t['荒庄-0']\t\t荒庄
0\t['W8','B7','W8','F4','W9','T2','T4','T7','T8','T1','W5','T2','T8','J3']\t1
1\t['W7','W7','B7','B6','B7','T3','F2','B8','W3','B2','W8','B3','W1']\t0
2\t['J3','W2','W8','T1','W9','B5','T3','F2','B6','T6','T4','J3','T9']\t0
3\t['W2','W4','T5','T8','T9','B1','W1','B7','J1','B5','T7','B2','W6']\t0
0\t打牌\t['J3']\t
2\t碰\t['J3','J3','J3']\tJ3\t0
2\t打牌\t['T9']\t
3\t摸牌\t['T6']\t
3\t打牌\t['J1']\t
0\t摸牌\t['H3']\t
0\t补花\t['H3']\t
0\t补花后摸牌\t['B4']\t
0\t打牌\t['B7']\t
1\t吃\t['B2','B3','B4']\tB4\t0
1\t打牌\t['T3']\t
";
}
