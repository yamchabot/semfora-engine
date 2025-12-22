//! NestJS Framework Detector
//!
//! Specialized extraction for NestJS applications including:
//! - @Injectable() decorated services
//! - @Controller() decorated controllers
//! - @Module() decorated modules
//! - HTTP method decorators (@Get, @Post, @Put, @Delete, etc.)
//! - Bootstrap function detection (main.ts)
//! - Dependency injection patterns

use crate::detectors::common::push_unique_insertion;
use crate::schema::{FrameworkEntryPoint, SemanticSummary, SymbolKind};

/// Enhance semantic summary with NestJS-specific information
///
/// This is called when NestJS is detected in the file.
pub fn enhance(summary: &mut SemanticSummary, source: &str) {
    let file_lower = summary.file.to_lowercase();

    // Detect NestJS patterns
    detect_decorators(summary, source);
    detect_bootstrap(summary, &file_lower, source);
    detect_module_patterns(summary, source);

    // Propagate framework entry point to symbols
    super::propagate_entry_point_to_symbols(summary);
}

/// Detect NestJS decorator patterns
fn detect_decorators(summary: &mut SemanticSummary, source: &str) {
    // @Injectable() - Services
    if source.contains("@Injectable(") {
        summary.framework_entry_point = FrameworkEntryPoint::NestService;
        push_unique_insertion(
            &mut summary.insertions,
            "NestJS injectable service".to_string(),
            "NestJS service",
        );

        // Mark decorated classes as services
        for symbol in &mut summary.symbols {
            if symbol.kind == SymbolKind::Class && symbol.decorators.iter().any(|d| d.contains("Injectable")) {
                symbol.framework_entry_point = FrameworkEntryPoint::NestService;
            }
        }
    }

    // @Controller() - Controllers
    if source.contains("@Controller(") {
        summary.framework_entry_point = FrameworkEntryPoint::NestController;
        push_unique_insertion(
            &mut summary.insertions,
            "NestJS controller".to_string(),
            "NestJS controller",
        );

        // Mark decorated classes as controllers
        for symbol in &mut summary.symbols {
            if symbol.kind == SymbolKind::Class && symbol.decorators.iter().any(|d| d.contains("Controller")) {
                symbol.framework_entry_point = FrameworkEntryPoint::NestController;
            }
        }

        // Also mark HTTP handler methods
        mark_http_handlers(summary, source);
    }

    // @Module() - Modules
    if source.contains("@Module(") {
        summary.framework_entry_point = FrameworkEntryPoint::NestModule;
        push_unique_insertion(
            &mut summary.insertions,
            "NestJS module".to_string(),
            "NestJS module",
        );

        // Mark decorated classes as modules
        for symbol in &mut summary.symbols {
            if symbol.kind == SymbolKind::Class && symbol.decorators.iter().any(|d| d.contains("Module")) {
                symbol.framework_entry_point = FrameworkEntryPoint::NestModule;
            }
        }
    }

    // Guard, Interceptor, Pipe, Filter - all are injectable
    for decorator in ["@UseGuards(", "@UseInterceptors(", "@UsePipes(", "@UseFilters("] {
        if source.contains(decorator) {
            push_unique_insertion(
                &mut summary.insertions,
                format!("NestJS {} middleware", decorator.trim_start_matches('@').trim_end_matches('(')),
                "NestJS middleware",
            );
        }
    }
}

/// Mark HTTP method handlers as framework entry points
fn mark_http_handlers(summary: &mut SemanticSummary, source: &str) {
    let http_decorators = ["@Get(", "@Post(", "@Put(", "@Delete(", "@Patch(", "@Head(", "@Options(", "@All("];

    for symbol in &mut summary.symbols {
        if symbol.kind == SymbolKind::Method || symbol.kind == SymbolKind::Function {
            // Check if method has HTTP decorator
            for decorator in &http_decorators {
                if symbol.decorators.iter().any(|d| d.contains(&decorator[1..decorator.len()-1])) {
                    symbol.framework_entry_point = FrameworkEntryPoint::NestController;
                    break;
                }
            }
        }
    }

    // Detect HTTP methods from source for insertion
    let mut methods = Vec::new();
    for decorator in &http_decorators {
        if source.contains(decorator) {
            let method = decorator.trim_start_matches('@').trim_end_matches('(');
            methods.push(method);
        }
    }

    if !methods.is_empty() {
        push_unique_insertion(
            &mut summary.insertions,
            format!("HTTP handlers: {}", methods.join(", ")),
            "HTTP handlers",
        );
    }
}

/// Detect NestJS bootstrap function (main.ts)
fn detect_bootstrap(summary: &mut SemanticSummary, file_lower: &str, source: &str) {
    // main.ts with NestFactory.create pattern
    if (file_lower.ends_with("/main.ts") || file_lower.ends_with("/main.js"))
        && source.contains("NestFactory")
    {
        summary.framework_entry_point = FrameworkEntryPoint::NestBootstrap;
        push_unique_insertion(
            &mut summary.insertions,
            "NestJS application bootstrap".to_string(),
            "NestJS bootstrap",
        );

        // Mark bootstrap function
        for symbol in &mut summary.symbols {
            if symbol.name == "bootstrap" {
                symbol.framework_entry_point = FrameworkEntryPoint::NestBootstrap;
            }
        }
    }
}

/// Detect NestJS module configuration patterns
fn detect_module_patterns(summary: &mut SemanticSummary, source: &str) {
    // Dynamic modules
    if source.contains("forRoot(") || source.contains("forRootAsync(") {
        push_unique_insertion(
            &mut summary.insertions,
            "dynamic module configuration".to_string(),
            "dynamic module",
        );
    }

    // Feature modules
    if source.contains("forFeature(") || source.contains("forFeatureAsync(") {
        push_unique_insertion(
            &mut summary.insertions,
            "feature module registration".to_string(),
            "feature module",
        );
    }

    // ConfigModule patterns
    if source.contains("ConfigModule") {
        push_unique_insertion(
            &mut summary.insertions,
            "configuration module".to_string(),
            "ConfigModule",
        );
    }

    // TypeORM patterns
    if source.contains("TypeOrmModule") {
        push_unique_insertion(
            &mut summary.insertions,
            "TypeORM database integration".to_string(),
            "TypeORM",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_detection() {
        let mut summary = SemanticSummary::default();
        let source = r#"
            @Controller('users')
            export class UsersController {
                @Get()
                findAll() { }

                @Post()
                create() { }
            }
        "#;

        enhance(&mut summary, source);

        assert_eq!(summary.framework_entry_point, FrameworkEntryPoint::NestController);
        assert!(summary.insertions.iter().any(|i| i.contains("NestJS controller")));
        assert!(summary.insertions.iter().any(|i| i.contains("HTTP handlers")));
    }

    #[test]
    fn test_service_detection() {
        let mut summary = SemanticSummary::default();
        let source = r#"
            @Injectable()
            export class UsersService {
                findAll() { }
            }
        "#;

        enhance(&mut summary, source);

        assert_eq!(summary.framework_entry_point, FrameworkEntryPoint::NestService);
        assert!(summary.insertions.iter().any(|i| i.contains("injectable service")));
    }

    #[test]
    fn test_bootstrap_detection() {
        let mut summary = SemanticSummary::default();
        summary.file = "/server/src/main.ts".to_string();
        let source = r#"
            async function bootstrap() {
                const app = await NestFactory.create(AppModule);
                await app.listen(3000);
            }
            bootstrap();
        "#;

        enhance(&mut summary, source);

        assert_eq!(summary.framework_entry_point, FrameworkEntryPoint::NestBootstrap);
        assert!(summary.insertions.iter().any(|i| i.contains("bootstrap")));
    }

    #[test]
    fn test_module_detection() {
        let mut summary = SemanticSummary::default();
        let source = r#"
            @Module({
                imports: [TypeOrmModule.forRoot()],
                controllers: [UsersController],
                providers: [UsersService],
            })
            export class AppModule { }
        "#;

        enhance(&mut summary, source);

        assert_eq!(summary.framework_entry_point, FrameworkEntryPoint::NestModule);
        assert!(summary.insertions.iter().any(|i| i.contains("NestJS module")));
    }
}
