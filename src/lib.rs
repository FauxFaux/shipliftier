extern crate cast;
#[macro_use]
extern crate failure;
extern crate heck;
extern crate mime;
extern crate result;
extern crate yaml_rust;

mod float;
mod render;
mod swagger;

use std::collections::HashMap;
use std::collections::HashSet;

use failure::Error;
use failure::ResultExt;

use swagger::name::Defs;
use swagger::Endpoint;
use swagger::Field;
use swagger::FullType;
use swagger::NamedType;
use swagger::StructContext;

pub fn go() -> Result<(), Error> {
    let doc = yaml_rust::YamlLoader::load_from_str(include_str!("../examples/docker.yaml"))?;
    let doc = &doc[0];

    let (mut def_names, endpoints) = swagger::name::definitions(
        doc["definitions"]
            .as_hash()
            .ok_or_else(|| format_err!("no definitions"))?,
        doc["paths"]
            .as_hash()
            .ok_or_else(|| format_err!("no paths"))?,
    )
    .with_context(|_| format_err!("processing definitions"))?;

    let endpoints = endpoints
        .into_iter()
        .map(|e| {
            e.map_type(|t, name_hints| {
                extract_names(&t, &name_hints, &mut def_names);

                Ok(t)
            })
        })
        .collect::<Result<Vec<Endpoint<FullType>>, Error>>()?;

    let mut banned_names = HashSet::new();
    'trying: loop {
        let mut used = HashSet::new();
        for possible_names in def_names.values() {
            let chosen = first_not_in(possible_names, &banned_names)?;

            if used.contains(chosen) {
                banned_names.insert(chosen.to_string());
                continue 'trying;
            }

            used.insert(chosen);
        }

        break;
    }

    let name_lookup = def_names
        .into_iter()
        .map(|(field, possible_names)| {
            first_not_in(&possible_names, &banned_names).map(|new| (field, new.to_string()))
        })
        .collect::<Result<HashMap<Vec<Field<FullType>>, String>, Error>>()?;

    let endpoints = endpoints
        .into_iter()
        .map(|e| e.map_type(|t, _| Ok(name_type(t, &name_lookup))))
        .collect::<Result<Vec<Endpoint<NamedType>>, Error>>()?;

    let mut render_order = name_lookup
        .iter()
        .map(|(k, v)| (v, k))
        .collect::<Vec<(&String, &Vec<Field<FullType>>)>>();

    render_order.sort_by_key(|(k, _)| k.to_string());

    for (name, fields) in render_order {
        use heck::MixedCase;

        println!("struct {} {{", name);
        for field in fields {
            println!(
                "    {}: {},",
                field.name.to_mixed_case(),
                render::render(&name_type(field.data_type.clone(), &name_lookup))
            );
        }
        println!("}}");
    }

    Ok(())
}

fn extract_names(
    t: &FullType,
    name_hints: &StructContext,
    def_names: &mut HashMap<Vec<Field<FullType>>, Vec<String>>,
) {
    match t {
        FullType::Fields(fields) => {
            def_names
                .entry(fields.clone())
                .or_insert_with(|| Vec::new())
                .extend(name_hints.recommended_names());

            for field in fields {
                let mut name_hints = name_hints.clone();
                name_hints.id = name_hints.id.map(|id| format!("{}{}", id, field.name));
                extract_names(&field.data_type, &name_hints, def_names);
            }
        }
        // TODO: could add extra hints here that we're in an array,
        // TODO: not really expecting multi-level arrays to be relevant
        FullType::Array { tee, .. } => extract_names(tee, name_hints, def_names),
        FullType::Simple(_) | FullType::Unknown => (),
    }
}

fn name_type(t: FullType, names: &HashMap<Vec<Field<FullType>>, String>) -> NamedType {
    match t {
        FullType::Fields(fields) => NamedType::Name(names[&fields].to_string()),
        FullType::Array { tee, constraints } => NamedType::Array {
            tee: Box::new(name_type(*tee, names)),
            constraints,
        },
        FullType::Simple(simple) => NamedType::Simple(simple),
        FullType::Unknown => NamedType::Unknown,
    }
}

fn first_not_in<'s>(
    container: &'s [String],
    blacklist: &HashSet<String>,
) -> Result<&'s String, Error> {
    container
        .iter()
        .filter(|n| !blacklist.contains(*n))
        .next()
        .ok_or_else(|| format_err!("No remaining names: {:?}", container))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        ::go().unwrap()
    }
}
