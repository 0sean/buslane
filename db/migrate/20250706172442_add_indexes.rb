class AddIndexes < ActiveRecord::Migration[8.0]
  def change
    add_index :routes, [ :agency_id, :id ]
    add_index :trips, [ :route_id, :id ]
  end
end
