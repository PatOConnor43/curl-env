use clap::{Parser, Subcommand};
use indexmap::IndexMap;
use openapiv3::{Components, Parameter, ReferenceOr, RequestBody, Response, Schema};
use std::{collections::BTreeMap, fs::File, io::Write, path::PathBuf, process::Command};

/// A command line tool that processes OpenAPI specifications
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Complete {
        /// Path to the OpenAPI specification file
        #[arg(short, long, value_name = "FILE")]
        spec: PathBuf,

        /// Optional prefix added to paths in the OpenAPI specification
        ///
        /// This is helpful when the OpenAPI spec is not at the root of the host. This prefix MUST
        /// start with a slash and not end with a slash.
        #[arg(short, long)]
        path_prefix: Option<String>,
    },
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    match args.command {
        Commands::Complete { spec, path_prefix } => complete(spec, path_prefix),
    }
}
fn complete(spec_path: PathBuf, path_prefix: Option<String>) -> Result<(), anyhow::Error> {
    if !spec_path.exists() {
        return Err(anyhow::anyhow!("Spec file does not exist"));
    }
    let spec_content = std::fs::read_to_string(&spec_path);
    if let Err(e) = spec_content {
        return Err(anyhow::anyhow!(
            "Error: Failed to read specification file: {}",
            e.to_string()
        ));
    }
    let spec_content = spec_content.unwrap();
    if spec_content.is_empty() {
        return Err(anyhow::anyhow!("Specification file is empty"));
    }
    // For JSON spec
    let spec: Result<openapiv3::OpenAPI, anyhow::Error> = if spec_path.ends_with(".json") {
        serde_json::from_str(&spec_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON OpenAPI spec: {}", e))
    } else {
        // For YAML spec
        serde_yaml::from_str(&spec_content)
            .map_err(|e| anyhow::anyhow!("Failed to parse YAML OpenAPI spec: {}", e))
    };
    let spec = spec?;
    let complete_urls = spec
        .paths
        .iter()
        .map(|(path, _)| {
            format!(r#"'http://localhost:9000{}'"#, path)
                .replace(":", "\\:")
                .replace("{", "")
                .replace("}", "")
        })
        .collect::<Vec<_>>();
    let mut query_options_vec: Vec<String> = vec![];
    for (path, method, op) in spec.operations() {
        let parameters = parameter_map(&op.parameters, &spec.components);
        if parameters.is_err() {
            continue;
        }
        let parameters = parameters.unwrap();
        let query_parameter_names = parameters.iter().filter_map(|(_, parameter)| {
            if let Parameter::Query { parameter_data, .. } = parameter {
                return Some(parameter_data.name.clone());
            }
            None
        });

        let options = query_parameter_names
            .map(|name| format!(r#"'{}='"#, name))
            .collect::<Vec<_>>()
            .join("\n                      ");
        let mut replaced_path = path.to_string();
        while replaced_path.contains('{') {
            let start = replaced_path.find('{').unwrap();
            let end = replaced_path.find('}').unwrap();
            replaced_path.replace_range(start..end + 1, r#"[[:alnum:]-]+"#);
        }
        replaced_path.push('$');
        query_options_vec.push(format!(
                r#"
                if [[ $current_url =~ http://localhost:9000{path} && $current_method == {method} ]]; then
                    raw_values=(
                      {options}
                    )
                fi
            "#
            ,path = replaced_path, method = method.to_uppercase(), options = options));
    }

    let xdg_dirs = xdg::BaseDirectories::with_prefix("curl-env");
    let data_dir = xdg_dirs.get_data_home().unwrap();
    let current_zshrc = xdg_dirs.get_data_file(".zshrc");
    let current_zshrc = match current_zshrc {
        None => {
            let path = xdg_dirs
                .place_data_file(".zshrc")
                .expect("Failed to get .zshrc file path");
            File::create(path.clone()).expect("Failed to create .zshrc file");
            path
        }
        Some(path) => {
            if !path.exists() {
                let path = xdg_dirs
                    .place_data_file(".zshrc")
                    .expect("Failed to get .zshrc file path");
                File::create(path.clone()).expect("Failed to create .zshrc file");
            }
            path
        }
    };

    let mut file = File::options()
        .truncate(true)
        .write(true)
        .open(current_zshrc)
        .expect("Failed to open .zshrc file");
    write!(
        file,
        "#
source $HOME/.zshrc

autoload -U is-at-least
#autoload -U compinit
#compinit


_custom_curl() {{
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext=\"$curcontext\" state line
    local current_url=\"\"
    local current_method=\"GET\"


    # Parse current command to extract URL and method
    for ((i = 2; i <= CURRENT; i++)); do
        if [[ ${{words[i]}} == -X || ${{words[i]}} == --request ]]; then
            (( i++ ))
            [[ $i -le CURRENT ]] && current_method=${{words[i]}}
        elif [[ ${{words[i]}} != -* && ${{words[i-1]}} != -* ]]; then
            # This might be the URL
            [[ ${{words[i]}} == http* ]] && current_url=${{words[i]}}
        fi
    done

    _arguments \"${{_arguments_options[@]}}\" \
        '(-h --help)'{{-h,--help}}'[Display help]' \
        '{{-H,--header}}'*'[Pass custom header]:header:->headers' \
        '(-o --output)'{{-o,--output}}'[Write output to file]:output file:_files' \
        '(-d --data)'{{-d,--data}}'[Pass request body]:body:->bodies' \
        '(-X --request)'{{-X,--request}}'[Specify request method]:method:(GET POST PUT DELETE PATCH)' \
        '*--data-urlencode[Specify query parameter]:query:->queries' \
        '(-G --get)'{{-G,--get}}'[Append request body as query parameters]' \
        '(-v --verbose)'{{-v,--verbose}}'[Verbose mode]' \
        '*:URL:->urls' \
        && ret=0
    
    case $state in
        urls)
            local -a my_urls
            my_urls=(
            {urls}
            )
            _describe -t urls \"URLs\" my_urls && ret=0
            ;;
        headers)
            local -a header_options
            if [[ $current_url =~ http://localhost:9000/platform/v1/documents/[[:alnum:]-]+$ ]]; then
                header_options=(
                    'Authorization\\:Bearer'
                )
            fi

            _describe -t headers \"Headers\" header_options && ret=0
            ;;
        queries)
            local -a raw_values
            {query_options}
            
            compadd -- \"${{raw_values[@]}}\" && ret=0
            ;;
        bodies)
            local -a body_options
            if [[ $current_url =~ http://localhost:9000/platform/v1/documents$ && $current_method == \"POST\" ]]; then
                body_options=(
                  \"'{{\\\"name\\\"\\: \\\"\\\"}}'\"
                )
            fi

            _describe -t bodies \"Request Bodies\" body_options -Q && ret=0
            ;;
    esac
    
    return ret

}}


if [ \"$funcstack[1]\" = \"_custom_curl\" ]; then
    _custom_curl \"$@\"
else
    compdef _custom_curl curl
fi
",
        urls = complete_urls.join("\n"),
        query_options = query_options_vec.join("\n")
    )?;
    Command::new("zsh")
        .env("ZDOTDIR", data_dir.to_string_lossy().trim_end_matches('/'))
        .status()
        .expect("Failed to execute zsh");

    Ok(())
}

pub(crate) trait ReferenceOrExt<T: ComponentLookup> {
    fn item<'a>(&'a self, components: &'a Option<Components>) -> anyhow::Result<&'a T>;
}
pub(crate) trait ComponentLookup: Sized {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>>;
}
impl<T: ComponentLookup> ReferenceOrExt<T> for openapiv3::ReferenceOr<T> {
    fn item<'a>(&'a self, components: &'a Option<Components>) -> anyhow::Result<&'a T> {
        match self {
            ReferenceOr::Item(item) => Ok(item),
            ReferenceOr::Reference { reference } => {
                let idx = reference.rfind('/').unwrap();
                let key = &reference[idx + 1..];
                let parameters = T::get_components(components.as_ref().unwrap());
                parameters
                    .get(key)
                    .unwrap_or_else(|| panic!("key {} is missing", key))
                    .item(components)
            }
        }
    }
}

pub(crate) fn items<'a, T>(
    refs: &'a [ReferenceOr<T>],
    components: &'a Option<Components>,
) -> impl Iterator<Item = anyhow::Result<&'a T>>
where
    T: ComponentLookup,
{
    refs.iter().map(|r| r.item(components))
}

pub(crate) fn parameter_map<'a>(
    refs: &'a [ReferenceOr<Parameter>],
    components: &'a Option<Components>,
) -> anyhow::Result<BTreeMap<&'a String, &'a Parameter>> {
    items(refs, components)
        .map(|res| res.map(|param| (&param.parameter_data_ref().name, param)))
        .collect()
}

impl ComponentLookup for Parameter {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>> {
        &components.parameters
    }
}

impl ComponentLookup for RequestBody {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>> {
        &components.request_bodies
    }
}

impl ComponentLookup for Response {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>> {
        &components.responses
    }
}

impl ComponentLookup for Schema {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>> {
        &components.schemas
    }
}
