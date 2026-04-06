// Need DB schema in code

fn main() {
    // #1 - Connect to Postgres
        // What's the best library to use? Probably tokio-postgres?
    
    // #2 - "Acquire an advisory lock"
        // How do I do that?

    // #3 - Ensure the directory exists

    // #4 - For each file, load the CSV into the staging tables

    // #5 - "Transform staging tables into final typed tables with SQL"
        // How do I do that?
    
    // #6 - "Build indexes after data load, not during it"
        // How do I do that?

    // #7 - Validation checks
        // How will these work? How do they work currently?

    // #8 - Swap the tables

    // #9 - Delete old tables
}
