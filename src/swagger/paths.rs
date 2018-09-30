use cast::u16;
use failure::Error;
use mime::Mime;
use yaml_rust::yaml::Hash;

use super::*;

pub fn paths(paths: &Hash) -> Result<Vec<Endpoint<PartialType>>, Error> {
    let mut ret = Vec::new();
    for (path_url, path) in paths.into_iter() {
        let path_url: &str = path_url
            .as_str()
            .ok_or_else(|| format_err!("non-string path url: {:?}", path_url))?;
        let path: &Hash = path
            .as_hash()
            .ok_or_else(|| format_err!("non-hash path body"))?;

        ret.push(Endpoint {
            path_url: path_url.to_string(),
            ops: process_methods(path)
                .with_context(|_| format_err!("processing path: {:?}", path_url))?,
        });
    }
    Ok(ret)
}

fn process_methods(path: &Hash) -> Result<HashMap<HttpMethod, Operation<PartialType>>, Error> {
    let mut ret = HashMap::new();

    for (http_method, op) in path.into_iter() {
        let http_method = match http_method
            .as_str()
            .ok_or_else(|| format_err!("non-string http method: {:?}", http_method))?
        {
            "get" => HttpMethod::GET,
            "post" => HttpMethod::POST,
            "head" => HttpMethod::HEAD,
            "put" => HttpMethod::PUT,
            "delete" => HttpMethod::DELETE,
            other => bail!("unsupported http method: {}", other),
        };

        ret.insert(
            http_method,
            process_method(
                op.as_hash()
                    .ok_or_else(|| format_err!("non-hash op body: {:?}", op))?,
            )
            .with_context(|_| format_err!("processing {:?}", http_method))?,
        );
    }

    Ok(ret)
}

fn process_method(op: &Hash) -> Result<Operation<PartialType>, Error> {
    let mut current_keys = keys(op)?;

    let mut params = Vec::new();
    if current_keys.remove("parameters") {
        for param in get_vec(op, "parameters")? {
            params.push(process_param(
                param
                    .as_hash()
                    .ok_or_else(|| format_err!("non-hash parameter"))?,
            )?);
        }
    }

    let mut responses = HashMap::new();

    current_keys.remove("responses");
    for (code, resp) in get_hash(op, "responses")? {
        let code = u16(as_integer(code)?)?;
        let resp: &Hash = resp
            .as_hash()
            .ok_or_else(|| format_err!("non-hash response {}", code))?;
        responses.insert(
            code,
            process_response(resp).with_context(|_| format_err!("response {}", code))?,
        );
    }

    current_keys.remove("summary"); // TODO
    current_keys.remove("description"); // TODO
    current_keys.remove("tags"); // TODO

    let consumes = if current_keys.remove("consumes") {
        get_vec(op, "consumes")?
            .into_iter()
            .map(as_mime)
            .collect::<Result<Vec<Mime>, Error>>()?
    } else {
        Vec::new()
    };

    let produces = if current_keys.remove("produces") {
        get_vec(op, "produces")?
            .into_iter()
            .map(as_mime)
            .collect::<Result<Vec<Mime>, Error>>()?
    } else {
        Vec::new()
    };

    current_keys.remove("operationId");

    ensure!(
        current_keys.is_empty(),
        "unrecognised keys: {:?}",
        current_keys
    );

    Ok(Operation {
        id: get_string(op, "operationId")?.to_string(),
        consumes,
        produces,
        params,
        responses,
    })
}

fn as_mime(yaml: &Yaml) -> Result<Mime, Error> {
    Ok(as_str(yaml)?.parse()?)
}

fn process_param(param: &Hash) -> Result<Param<PartialType>, Error> {
    let mut current_keys = keys(param)?;

    current_keys.remove("name");
    current_keys.remove("in");

    let name = get_string(param, "name")?;
    let loc = match get_string(param, "in")? {
        "query" => ParamLocation::Query,
        "body" => ParamLocation::Body,
        "path" => ParamLocation::Path,
        "header" => ParamLocation::Header,
        other => bail!("invalid `in` location: {}", other),
    };

    let description = if current_keys.remove("description") {
        get_string(param, "description")?
    } else {
        ""
    };

    let required = if current_keys.remove("required") {
        Some(get_bool(param, "required")?)
    } else {
        None
    };

    let param_type = if current_keys.remove("schema") {
        let schema = get_hash(param, "schema")?;
        let mut schema_keys = keys(schema)?;
        schema_keys.remove("example");

        // TODO: could these be useful?
        schema_keys.remove("title");
        schema_keys.remove("description");

        let field_result = definitions::field_type(schema, &mut schema_keys);

        ensure!(
            schema_keys.is_empty(),
            "unrecognised schema keys: {:?}",
            schema_keys
        );

        field_result
    } else {
        definitions::field_type(param, &mut current_keys)
    }
    .with_context(|_| format_err!("finding type of {:?}", name))?;

    ensure!(
        current_keys.is_empty(),
        "{:?}: unrecognised keys: {:?}",
        name,
        current_keys
    );

    Ok(Param {
        name: name.to_string(),
        loc,
        description: description.to_string(),
        required,
        param_type,
    })
}

fn process_response(resp: &Hash) -> Result<Response<PartialType>, Error> {
    let mut current_keys = keys(resp)?;
    let description = if current_keys.remove("description") {
        get_string(resp, "description")?.to_string()
    } else {
        String::new()
    };
    current_keys.remove("examples");

    let mut headers = HashMap::new();

    if current_keys.remove("headers") {
        for (header_name, header) in get_hash(resp, "headers")? {
            let header_name = as_str(header_name)?.to_string();
            let header = header
                .as_hash()
                .ok_or_else(|| format_err!("non-hash header: {:?}", header))?;
            let mut header_keys = keys(header)?;
            header_keys.remove("description");
            let header_type = definitions::field_type(header, &mut header_keys)?;
            ensure!(
                header_keys.is_empty(),
                "unsupported header keys: {:?}",
                header_keys
            );

            if let PartialType::Simple(header_type) = header_type {
                headers.insert(header_name, Header { header_type });
            } else {
                bail!("headers must be simple types, not {:?}", header_type);
            }
        }
    }

    let resp_type = if current_keys.remove("schema") {
        let schema = get_hash(resp, "schema")?;
        let mut schema_keys = keys(schema)?;
        Some(definitions::field_type(schema, &mut schema_keys)?)
    } else {
        None
    };

    ensure!(
        current_keys.is_empty(),
        "unrecognised response body keys: {:?}",
        current_keys
    );

    Ok(Response {
        description,
        headers,
        resp_type,
    })
}
