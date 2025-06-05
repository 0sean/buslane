class CreateRoutes < ActiveRecord::Migration[8.0]
  def change
    create_table :routes, id: false, force: true do |t|
      t.integer :id, null: false, primary_key: true
      t.references :agency, null: false, type: :string
      t.string :short_name
      t.integer :long_name
      t.integer :route_type, null: false

      t.timestamps
    end
  end
end
