// cargo run --release -p eqoa --example dict_overlap  (reads run/eqoa.iso)
// How many DictIDs do the shared files and a world have in common, and do the
// shared ones carry the same bytes? (Which file wins changes what is drawn.)
fn main() {
    let mut iso = iso::Iso::open("run/eqoa.iso").unwrap();
    let names = ["TUNARIA.ESF", "CHAR.ESF", "CHARCUST.CSF", "CHARFACE.CSF", "ITEM.ESF", "UI.CSF", "SPELLFX.CSF"];
    let files: Vec<(&str, esf::Container)> = names
        .iter()
        .filter_map(|n| Some((*n, esf::Container::open(iso.read(n).ok()?).ok()?)))
        .collect();
    for i in 0..files.len() {
        for j in i + 1..files.len() {
            let (a, b) = (&files[i].1, &files[j].1);
            let common: Vec<i32> = a.dict.keys().filter(|k| b.dict.contains_key(k)).copied().collect();
            if common.is_empty() {
                continue;
            }
            let differ = common
                .iter()
                .filter(|&&k| {
                    let (na, nb) = (a.node(a.dict[&k]), b.node(b.dict[&k]));
                    a.data[na.body..na.body + na.size] != b.data[nb.body..nb.body + nb.size]
                })
                .count();
            println!("{} / {}: {} shared DictIDs, {differ} with different bytes", files[i].0, files[j].0, common.len());
        }
    }
}
