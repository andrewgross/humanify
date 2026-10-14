//! usage: exp110-reencode <units.jsonl> <packages.json> <out units.bin>
use humanify_core::libindex::format::UnitRow;
use humanify_core::libindex::table::UnitTable;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1]).expect("rows");
    let rows: Vec<UnitRow> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    let packages: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(&a[2]).expect("packages")).expect("json");
    let bytes = UnitTable::encode(&rows, &packages).expect("encodes");
    std::fs::write(&a[3], bytes).expect("write");
    eprintln!("{} rows -> {}", rows.len(), a[3]);
}
