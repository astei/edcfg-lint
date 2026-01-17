use regex::RegexSet;
use std::sync::OnceLock;

static DEFAULT_EXCLUDES: OnceLock<RegexSet> = OnceLock::new();

pub fn get_default_excludes() -> &'static RegexSet {
    DEFAULT_EXCLUDES.get_or_init(|| {
        let patterns = vec![
            // source control related files and folders
            r"\.git/",
            r"\.jj/",
            // package manager, generated, & lock files
            // Cargo (Rust)
            r"Cargo\.lock$",
            r"/target/",
            // Composer (PHP)
            r"composer\.lock$",
            // RubyGems (Ruby)
            r"Gemfile\.lock$",
            // Go Modules (Go)
            r"go\.(mod|sum|work|work\.sum)$",
            // Gradle (Java)
            r"gradle/wrapper/gradle-wrapper\.properties$",
            r"gradlew(\.bat)?$",
            r"(buildscript-)?gradle\.lockfile?$",
            // Maven (Java)
            r"\.mvn/wrapper/maven-wrapper\.properties$",
            r"\.mvn/wrapper/MavenWrapperDownloader\.java$",
            r"mvnw(\.cmd)?$",
            // NodeJS
            r"/node_modules/",
            // npm (NodeJS)
            r"npm-shrinkwrap\.json$",
            r"package-lock\.json$",
            // pip (Python)
            r"Pipfile\.lock$",
            // Poetry (Python)
            r"poetry\.lock$",
            // pnpm (NodeJS)
            r"pnpm-lock\.yaml$",
            // Terraform & OpenTofu
            r"\.terraform\.lock\.hcl$",
            // uv (Python)
            r"uv\.lock$",
            // yarn (NodeJS)
            r"\.pnp\.c?js$",
            r"\.pnp\.loader\.mjs$",
            r"\.yarn/",
            r"yarn\.lock$",
            // font files
            r"\.eot$",
            r"\.otf$",
            r"\.ttf$",
            r"\.woff2?$",
            // image & video formats
            r"\.avif$",
            r"\.gif$",
            r"\.ico$",
            r"\.jpe?g$",
            r"\.pcm$",
            r"\.mp3$",
            r"\.mp4$",
            r"\.p[bgnp]m$",
            r"\.png$",
            r"\.svg$",
            r"\.tiff?$",
            r"\.webp$",
            r"\.wmv$",
            // other binary or container formats
            r"\.bak$",
            r"\.bin$",
            r"\.docx?$",
            r"\.exe$",
            r"\.pdf$",
            r"\.snap$",
            r"\.xlsx?$",
            // archive formats
            r"\.7z$",
            r"\.bz2$",
            r"\.gz$",
            r"\.jar$",
            r"\.tar$",
            r"\.tgz$",
            r"\.war$",
            r"\.zip$",
            // log & (git) patch files
            r"\.log$",
            r"\.patch$",
            // generated or minified CSS and JavaScript files
            r"\.(css|js)\.map$",
            r"min\.(css|js)$",
            // emacs backup files
            r"~$",
        ];
        RegexSet::new(patterns).expect("Failed to compile default exclude patterns")
    })
}
