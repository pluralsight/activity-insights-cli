use phf_codegen::Set;
use std::{
    error::Error,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

const PACKAGES_LIST: &'static str = "common-packages.txt";
const PACKAGES_CODEGEN: &'static str = "src/codegen/packages-set.rs";

fn main() -> Result<(), Box<dyn Error>> {
    let packages_file = File::open(PACKAGES_LIST)?;
    let packages = BufReader::new(packages_file).lines();
    let mut packages: Vec<_> = packages.map(|x| x.unwrap()).collect();
    packages.sort();
    packages.dedup();

    let mut package_set_builder: Set<&str> = Set::new();
    for package in packages.iter() {
        package_set_builder.entry(&package[..]);
    }

    let mut codegen_file = BufWriter::new(File::create(PACKAGES_CODEGEN)?);
    writeln!(
        &mut codegen_file,
        "static PACKAGES: phf::Set<&'static str> =\n{};\n",
        package_set_builder.build()
    )?;

    Ok(())
}
