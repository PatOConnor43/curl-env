use anyhow::Context;
use clap::{Parser, Subcommand};
use indexmap::IndexMap;
use openapiv3::{Components, Example, Parameter, ReferenceOr, RequestBody, Response, Schema};
use std::{collections::BTreeMap, fs::File, io::Write, path::PathBuf, process::Command};

/// A command line tool that autocompletes curl commands based on an OpenAPI specification
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Activates a subshell environment where the curl completion script is active
    Activate {
        /// Path to the OpenAPI specification file
        #[arg(long, value_name = "FILE")]
        spec: PathBuf,

        /// URL where the server is mounted
        ///
        /// This should be any http or https url where the OpenAPI routes are mounted.
        /// Examples:
        /// - http://localhost:9000
        /// - http://localhost:9000/api
        /// - https://api.example.com/v1
        /// - https://example.com/api/v2/
        #[arg(long, verbatim_doc_comment)]
        base_url: url::Url,

        /// Shell type to activate
        #[arg(value_enum, default_value_t = Shell::Zsh)]
        shell: Shell,
    },
    /// Generate shell completion scripts
    Completion {
        /// Path to the OpenAPI specification file
        #[arg(long, value_name = "FILE")]
        spec: PathBuf,

        /// URL where the server is mounted
        ///
        /// This should be any http or https url where the OpenAPI routes are mounted.
        /// Examples:
        /// - http://localhost:9000
        /// - http://localhost:9000/api
        /// - https://api.example.com/v1
        /// - https://example.com/api/v2/
        #[arg(long, verbatim_doc_comment)]
        base_url: url::Url,

        /// Shell type to generate completion for
        #[arg(value_enum, default_value_t = Shell::Zsh)]
        shell: Shell,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Shell {
    Zsh,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    match args.command {
        Commands::Activate {
            spec,
            base_url,
            shell,
        } => {
            let spec = read_spec_from_path(&spec)?;
            match shell {
                Shell::Zsh => {
                    let zsh_content = get_zsh_content(spec, base_url)?;
                    let zsh_path = get_zsh_path()?;
                    let mut file = File::options()
                        .truncate(true)
                        .write(true)
                        .open(&zsh_path)
                        .context("Failed to open .zshrc file")?;
                    file.write_all(zsh_content.as_bytes())
                        .context("Failed to write to .zshrc")?;
                    Command::new("zsh")
                        .env("ZDOTDIR", zsh_path.parent().unwrap())
                        .status()
                        .context("Failed to execute zsh")?;
                }
            }

            Ok(())
        }
        Commands::Completion {
            shell,
            spec,
            base_url,
        } => {
            let spec = read_spec_from_path(&spec)?;
            match shell {
                Shell::Zsh => {
                    let content = get_zsh_content(spec, base_url)?;
                    println!("{}", content);
                    Ok(())
                }
            }
        }
    }
}

fn get_zsh_path() -> Result<PathBuf, anyhow::Error> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("curl-env");
    let current_zshrc = xdg_dirs.get_data_file(".zshrc");
    match current_zshrc {
        None => {
            let path = xdg_dirs
                .place_data_file(".zshrc")
                .expect("Failed to get .zshrc file path");
            File::create(path.clone()).expect("Failed to create .zshrc file");
            Ok(path)
        }
        Some(path) => {
            if !path.exists() {
                let path = xdg_dirs
                    .place_data_file(".zshrc")
                    .expect("Failed to get .zshrc file path");
                File::create(path.clone()).expect("Failed to create .zshrc file");
            }
            Ok(path)
        }
    }
}

fn read_spec_from_path(spec_path: &PathBuf) -> Result<openapiv3::OpenAPI, anyhow::Error> {
    if !spec_path.exists() {
        return Err(anyhow::anyhow!("Spec file does not exist"));
    }
    let spec_content = std::fs::read_to_string(spec_path);
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
    Ok(spec)
}

fn get_zsh_content(spec: openapiv3::OpenAPI, base_url: url::Url) -> Result<String, anyhow::Error> {
    let mut base_url = base_url;
    base_url.set_query(None);

    let complete_urls = spec
        .paths
        .iter()
        .filter_map(|(path, _)| {
            let path = path.replace("{", "").replace("}", "");
            let url = base_url.join(path.as_str()).ok()?;
            Some(format!("'{}'", url.as_str()).replace(":", "\\:"))
        })
        .collect::<Vec<_>>();
    let mut query_options_vec: Vec<String> = vec![];

    // Collect query parameters
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
            .map(|name| format!(r#"$'\'{}=\''"#, name))
            .collect::<Vec<_>>()
            .join(format!("\n{}", " ".repeat(18)).as_str());

        if options.is_empty() {
            continue;
        }

        let mut replaced_path = path.to_string();
        while replaced_path.contains('{') {
            let start = replaced_path.find('{').unwrap();
            let end = replaced_path.find('}').unwrap();
            replaced_path.replace_range(start..end + 1, r#"[[:alnum:]_-]+"#);
        }
        replaced_path.push('$');
        let url = base_url.join(replaced_path.as_str());
        if url.is_err() {
            continue;
        }
        let url = url.unwrap();

        query_options_vec.push(format!(
            r#"
            if [[ $current_url =~ {url} && $current_method == {method} ]]; then
                query_options=(
                  {options}
                )
            fi"#,
            url = url,
            method = method.to_uppercase(),
            options = options
        ));
    }

    let mut body_options_vec: Vec<String> = vec![];
    // Collect request body examples
    for (path, method, op) in spec.operations() {
        match op.request_body.as_ref() {
            Some(body) => {
                let body = body.item(&spec.components)?;
                let content = body.content.get("application/json");
                if content.is_none() {
                    body_options_vec.push("".to_string());
                    continue;
                }
                let content = content.unwrap();
                let mut replaced_path = path.to_string();
                while replaced_path.contains('{') {
                    let start = replaced_path.find('{').unwrap();
                    let end = replaced_path.find('}').unwrap();
                    replaced_path.replace_range(start..end + 1, r#"[[:alnum:]_-]+"#);
                }
                replaced_path.push('$');
                if let Some(example) = &content.example {
                    let example = serde_json::to_string(example)?;
                    let example = example
                        .replace(r#"'"#, r#"\'"#)
                        .replace(r#":"#, r#"\:"#)
                        .replace(r#"$"#, r#"\$"#)
                        .replace(
                            r#"
"#, r#"\n"#,
                        );

                    let url = base_url.join(replaced_path.as_str());
                    if url.is_err() {
                        continue;
                    }
                    let url = url.unwrap();

                    body_options_vec.push(format!(
                        r#"
            if [[ $current_url =~ {url} && $current_method == {method} ]]; then
              body_options=(
                $'\$\'{example}\''
              )

              descriptions=(
                'Request Body Example'
              )
            fi"#,
                        url = url,
                        method = method.to_uppercase()
                    ));
                    continue;
                } else if !&content.examples.is_empty() {
                    let mut body_examples = vec![];
                    let mut body_example_descriptions = vec![];
                    for (name, example) in &content.examples {
                        let example = example.item(&spec.components)?;
                        if let Some(value) = &example.value {
                            let value = serde_json::to_string(value)?;
                            let value = value
                                .replace(r#"'"#, r#"\'"#)
                                .replace(r#":"#, r#"\:"#)
                                .replace(r#"$"#, r#"\$"#)
                                .replace(
                                    r#"
"#, r#"\n"#,
                                );
                            body_examples.push(format!(r#"$'\$\'{value}\''"#));
                            body_example_descriptions.push(format!(r#"'{}'"#, name));
                        }
                    }

                    let url = base_url.join(replaced_path.as_str());
                    if url.is_err() {
                        continue;
                    }
                    let url = url.unwrap();

                    body_options_vec.push(format!(
                        r#"
            if [[ $current_url =~ {url} && $current_method == {method} ]]; then
              body_options=(
                {body_examples}
              )
              descriptions=(
                {descriptions}
              )
            fi"#,
                        url = url,
                        method = method.to_uppercase(),
                        body_examples =
                            body_examples.join(format!("\n{}", " ".repeat(16)).as_str()),
                        descriptions = body_example_descriptions
                            .join(format!("\n{}", " ".repeat(16)).as_str())
                    ));
                }
            }
            None => body_options_vec.push("".to_string()),
        }
    }
    let body_options_vec: Vec<String> = body_options_vec
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();

    let zsh_content = format!(
        "#
autoload -U is-at-least
autoload -U compinit
compinit

_custom_curl_env() {{
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
            local -a url_options
            url_options=(
            {urls}
            )
            _describe -t urls \"URLs\" url_options && ret=0
            ;;
        headers)
            local -a header_options
            _describe -t headers \"Headers\" header_options && ret=0
            ;;
        queries)
            local -a query_options
            {query_options}

            if [[ -z \"$PREFIX\" || \"$PREFIX\" = '$' || \"$PREFIX\" = \"$'\" ]]; then
              # First tab press - show complete options
              compstate[insert]=menu   # Force menu completion
              compstate[list]=list     # Always show the list
              compadd -Q -X \"Query Parameters\" -- \"${{query_options[@]}}\"
            else
              # Get the currently typed text and match complete options only
              local current=\"$PREFIX$SUFFIX\"
              local -a matches=()

              for opt in \"${{query_options[@]}}\"; do
                if [[ \"$opt\" = \"$current\"* ]]; then
                  matches+=(\"$opt\")
                fi
              done

              if (( ${{#matches}} > 0 )); then
                compstate[insert]=all
                compadd -Q -- \"${{matches[@]}}\"
              fi
            fi
            ;;
        bodies)
            local -a body_options descriptions
            {body_options}

            # Check if we're at the beginning of completion or continuing
            if [[ -z \"$PREFIX\" || \"$PREFIX\" = '$' || \"$PREFIX\" = \"$'\" ]]; then
              # First tab press - show complete options
              compstate[insert]=menu   # Force menu completion
              compstate[list]=list     # Always show the list
              compadd -Q -X \"Request Body Examples\" -d descriptions -- \"${{body_options[@]}}\"
            else
              # Get the currently typed text and match complete options only
              local current=\"$PREFIX$SUFFIX\"
              local -a matches=()

              for opt in \"${{body_options[@]}}\"; do
                if [[ \"$opt\" = \"$current\"* ]]; then
                  matches+=(\"$opt\")
                fi
              done

              if (( ${{#matches}} > 0 )); then
                compstate[insert]=all
                compadd -Q -- \"${{matches[@]}}\"
              fi
            fi
            ;;
    esac
    return ret

}}


if [ \"$funcstack[1]\" = \"_custom_curl_env\" ]; then
    _custom_curl_env \"$@\"
else
    compdef _custom_curl_env curl
fi

",
        urls = complete_urls.join(format!("\n{}", " ".repeat(12)).as_str()),
        query_options = query_options_vec.join("\n"),
        body_options = body_options_vec.join("\n")
    );

    Ok(zsh_content)
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

impl ComponentLookup for Example {
    fn get_components(components: &Components) -> &IndexMap<String, ReferenceOr<Self>> {
        &components.examples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn petstore_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let spec = read_spec_from_path(&PathBuf::from("src/snapshots/petstore.yaml"))?;
        let base_url = url::Url::parse("https://petstore.swagger.io/v3")?;
        let zsh_content = get_zsh_content(spec, base_url)?;
        assert_snapshot!(zsh_content);
        Ok(())
    }
}
